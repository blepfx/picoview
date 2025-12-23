use super::display::*;
use super::util::{flags_to_modifiers, get_cursor, keycode_to_key, random_id};
use crate::platform::mac::util::{get_clipboard_text, set_clipboard_text, spawn_detached};
use crate::platform::{OpenMode, PlatformWaker, PlatformWindow};
use crate::{
    Error, Event, MouseButton, MouseCursor, Point, Size, WakeupError, Window, WindowBuilder,
    WindowWaker, rwh_06,
};
use objc2::rc::{Allocated, Retained, Weak, autoreleasepool};
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly};
use objc2::{
    ClassType, Encoding, Message, RefEncode,
    declare::ClassBuilder,
    ffi::objc_disposeClassPair,
    msg_send,
    runtime::{AnyClass, Bool},
    sel,
};
use objc2_app_kit::{
    NSApp, NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSCursor,
    NSDragOperation, NSDraggingInfo, NSEvent, NSPasteboardTypeFileURL, NSScreen, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindow, NSWindowDidBecomeKeyNotification,
    NSWindowDidResignKeyNotification, NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGSize};
use objc2_core_graphics::CGWarpMouseCursorPosition;
use objc2_foundation::{
    NSArray, NSNotification, NSNotificationCenter, NSObjectNSThreadPerformAdditions, NSPoint,
    NSRect, NSSize, NSString,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::{CString, c_void};
use std::ptr::NonNull;
use std::sync::Arc;

#[repr(C)]
pub struct WindowView {
    view: NSView,
}

struct WindowViewWaker {
    weak: Weak<WindowView>,
}

unsafe impl Send for WindowViewWaker {}
unsafe impl Sync for WindowViewWaker {}

struct WindowViewInner {
    _display_link: DisplayLink,
    application: RefCell<Option<Retained<NSApplication>>>,
    waker: Arc<WindowViewWaker>,

    event_queue: RefCell<VecDeque<Event<'static>>>,
    current_cursor: Cell<MouseCursor>,

    #[allow(clippy::type_complexity)]
    event_handler: RefCell<Option<Box<dyn FnMut(Event)>>>,

    is_closed: Cell<bool>,
}

unsafe impl RefEncode for WindowView {
    const ENCODING_REF: Encoding = NSView::ENCODING_REF;
}

unsafe impl Message for WindowView {}

impl WindowView {
    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<WindowWaker, Error> {
        match mode {
            OpenMode::Blocking => autoreleasepool(|_| unsafe {
                let main_thread = MainThreadMarker::new()
                    .ok_or_else(|| Error::PlatformError("not in main thread".into()))?;

                let app = NSApp(main_thread);
                app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

                let window = Self::create_window(&options, main_thread)?;
                let view = Self::create_view(options, Some(app.clone()))?;

                window.setContentView(Some(&view.view));
                //window.setDelegate(Some(&view));

                app.run();
                Ok(WindowWaker::default())
            }),

            OpenMode::Embedded(parent) => autoreleasepool(|_| unsafe {
                let parent_view = match parent {
                    rwh_06::RawWindowHandle::AppKit(window) => {
                        window.ns_view.as_ptr() as *mut NSView
                    }
                    _ => return Err(Error::InvalidParent),
                };

                let view = Self::create_view(options, None)?;
                (*parent_view).addSubview(&view.view);
                Ok(view.waker())
            }),
        }
    }

    unsafe fn create_window(
        options: &WindowBuilder,
        main_thread: MainThreadMarker,
    ) -> Result<Retained<NSWindow>, Error> {
        unsafe {
            let rect = NSRect::new(
                NSPoint::new(
                    options.position.map(|x| x.x).unwrap_or(0.0) as f64,
                    options.position.map(|x| x.y).unwrap_or(0.0) as f64,
                ),
                NSSize::new(options.size.width as f64, options.size.height as f64),
            );

            let mut style = NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable;

            if options.resizable.is_some() {
                style |= NSWindowStyleMask::Resizable;
            }

            let window = NSWindow::alloc(main_thread);
            let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                window,
                rect,
                style,
                NSBackingStoreType::Buffered,
                false,
            );

            if options.position.is_none() {
                window.center();
            }

            if options.visible {
                window.makeKeyAndOrderFront(None);
            }

            if let Some(range) = options.resizable.clone() {
                window.setContentMinSize(NSSize::new(
                    range.start.width as f64,
                    range.start.height as f64,
                ));
                window.setContentMaxSize(NSSize::new(
                    range.end.width as f64,
                    range.end.height as f64,
                ));
            }

            window.setTitle(&NSString::from_str(&options.title));

            Ok(window)
        }
    }

    unsafe fn create_view(
        options: WindowBuilder,
        blocking: Option<Retained<NSApplication>>,
    ) -> Result<Retained<Self>, Error> {
        let class = Self::register_class()?;

        let rect = NSRect::new(
            NSPoint {
                x: options.position.map(|x| x.x).unwrap_or_default() as f64,
                y: options.position.map(|x| x.y).unwrap_or_default() as f64,
            },
            NSSize {
                width: options.size.width.max(1) as f64,
                height: options.size.height.max(1) as f64,
            },
        );

        let view = unsafe {
            let view: Allocated<WindowView> = msg_send![class, alloc];
            let view: Retained<WindowView> = msg_send![view, initWithFrame: rect];

            let tracking_area = NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                rect,
                NSTrackingAreaOptions::MouseEnteredAndExited
                    | NSTrackingAreaOptions::MouseMoved
                    | NSTrackingAreaOptions::ActiveAlways
                    | NSTrackingAreaOptions::InVisibleRect,
                Some(&view.view),
                None,
            );

            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &view.view,
                sel!(picoview_handleNotification:),
                Some(NSWindowDidBecomeKeyNotification),
                None,
            );

            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &view.view,
                sel!(picoview_handleNotification:),
                Some(NSWindowDidResignKeyNotification),
                None,
            );

            let dragged_types = NSArray::arrayWithObject(NSPasteboardTypeFileURL);
            view.view.addTrackingArea(&tracking_area);
            view.view.registerForDraggedTypes(&dragged_types);
            view
        };

        let display = {
            let view = Weak::from_retained(&view);
            DisplayLink::new(Box::new(move || {
                if let Some(view) = view.load() {
                    view.send_event(Event::WindowFrame { gl: None });
                }
            }))?
        };

        view.set_inner(Box::new(WindowViewInner {
            _display_link: display,
            application: RefCell::new(blocking),

            waker: Arc::new(WindowViewWaker {
                weak: Weak::from_retained(&view),
            }),

            event_queue: RefCell::new(VecDeque::default()),
            event_handler: RefCell::new(None),

            current_cursor: Cell::new(MouseCursor::Default),
            is_closed: Cell::new(false),
        }));

        // SAFETY: we erase the lifetime of our OsWindowView; it should be safe to do so because:
        //  - because our window instance has a stable address for the whole lifetime of the window (due to being stored as Retained)
        //  - we manually dispose of our handler before WindowImpl gets dropped (see drop impl)
        //  - we promise to not move the handler to a different thread as appkit api is expected to be single threaded (as that would violate the handler's !Send requirement)
        unsafe {
            view.inner()
                .event_handler
                .replace(Some((options.factory)(Window(&*Retained::as_ptr(&view)))));
        }

        Ok(view)
    }

    fn send_event(&self, event: Event) {
        if let Ok(mut handler) = self.inner().event_handler.try_borrow_mut() {
            if let Some(handler) = handler.as_mut() {
                (handler)(event);

                while let Some(event) = self.inner().event_queue.borrow_mut().pop_front() {
                    (handler)(event);
                }
            }
        } else {
            debug_assert!(false, "send_event reentrancy")
        }
    }

    fn send_event_defer(&self, event: Event<'static>) {
        if self.inner().event_handler.try_borrow_mut().is_ok() {
            self.send_event(event);
        } else {
            self.inner().event_queue.borrow_mut().push_back(event);
        }
    }

    fn set_inner(&self, context: Box<WindowViewInner>) {
        unsafe {
            self.view
                .class()
                .instance_variable(c"_context")
                .unwrap_unchecked()
                .load_ptr::<*mut c_void>(&self.view)
                .write(Box::into_raw(context) as *mut c_void);
        }
    }

    fn inner(&self) -> &WindowViewInner {
        unsafe {
            let ivar = self
                .view
                .class()
                .instance_variable(c"_context")
                .unwrap_unchecked();
            let context = *ivar.load::<*mut c_void>(&self.view) as *mut WindowViewInner;
            assert!(!context.is_null());
            &*context
        }
    }

    // NSView
    unsafe extern "C" fn init_with_frame(&self, _cmd: Sel, rect: NSRect) -> Option<&Self> {
        unsafe { msg_send![super(self, NSView::class()), initWithFrame: rect] }
    }

    unsafe extern "C" fn dealloc(&self, _cmd: Sel) {
        unsafe {
            println!("dealloc begin");

            let ivar = self
                .view
                .class()
                .instance_variable(c"_context")
                .unwrap_unchecked();

            let context =
                *ivar.load::<*mut c_void>(&self.view) as *mut Box<RefCell<WindowViewInner>>;

            if !context.is_null() {
                println!("drop begin");
                drop(Box::from_raw(context));
                println!("drop end");
            }

            NSNotificationCenter::defaultCenter().removeObserver(&self.view);
            let _: () = msg_send![super(self, NSView::class()), dealloc];
            let class: &'static AnyClass = msg_send![self, class];
            objc_disposeClassPair(class as *const _ as *mut _);

            println!("dealloc end");
        }
    }

    unsafe extern "C" fn view_did_change_backing_properties(&self, _: Sel, _: Option<&AnyObject>) {
        let scale = self
            .view
            .window()
            .map(|x| x.backingScaleFactor())
            .unwrap_or(1.0);

        self.send_event_defer(Event::WindowScale {
            scale: scale as f32,
        });

        // TODO: fun logical -> physical scaling stuff here
    }

    unsafe extern "C" fn accepts_first_mouse(&self, _cmd: Sel, _event: *const NSEvent) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn accepts_first_responder(&self, _cmd: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn is_flipped(&self, _cmd: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn key_down(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            let mut capture = false;
            if let Some(key) = keycode_to_key((*event).keyCode()) {
                self.send_event(Event::KeyDown {
                    key,
                    capture: &mut capture,
                });
            }

            if !capture {
                msg_send![super(self, NSView::class()), keyDown: event]
            }
        }
    }

    unsafe extern "C" fn key_up(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            let mut capture = false;
            if let Some(key) = keycode_to_key((*event).keyCode()) {
                self.send_event(Event::KeyUp {
                    key,
                    capture: &mut capture,
                });
            }

            if !capture {
                msg_send![super(self, NSView::class()), keyUp: event]
            }
        }
    }

    unsafe extern "C" fn flags_changed(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            self.send_event_defer(Event::KeyModifiers {
                modifiers: flags_to_modifiers((*event).modifierFlags()),
            });
        }
    }

    unsafe extern "C" fn mouse_moved(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            let absolute = NSEvent::mouseLocation(); // TODO: fix flipped y coord
            let relative = (*event).locationInWindow();
            let relative = self.view.convertPoint_fromView(relative, None);

            self.send_event_defer(Event::MouseMove {
                relative: Point {
                    x: relative.x as f32,
                    y: relative.y as f32,
                },
                absolute: Point {
                    x: absolute.x as f32,
                    y: absolute.y as f32,
                },
            });
        }
    }

    unsafe extern "C" fn mouse_down(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            let button = match (*event).buttonNumber() {
                0 => Some(MouseButton::Left),
                1 => Some(MouseButton::Right),
                2 => Some(MouseButton::Middle),
                3 => Some(MouseButton::Back),
                4 => Some(MouseButton::Forward),
                _ => None,
            };

            if let Some(button) = button {
                self.send_event_defer(Event::MouseDown { button });
            }
        }
    }

    unsafe extern "C" fn mouse_up(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            let button = match (*event).buttonNumber() {
                0 => Some(MouseButton::Left),
                1 => Some(MouseButton::Right),
                2 => Some(MouseButton::Middle),
                3 => Some(MouseButton::Back),
                4 => Some(MouseButton::Forward),
                _ => None,
            };

            if let Some(button) = button {
                self.send_event_defer(Event::MouseUp { button });
            }
        }
    }

    unsafe extern "C" fn mouse_exited(&self, _cmd: Sel, _event: *const NSEvent) {
        self.send_event_defer(Event::MouseLeave);
    }

    unsafe extern "C" fn scroll_wheel(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            if event.is_null() {
                return;
            }

            let mut x = -(*event).scrollingDeltaX() as f32;
            let mut y = (*event).scrollingDeltaY() as f32;

            if (*event).hasPreciseScrollingDeltas() {
                x /= 10.0;
                y /= 10.0;
            }

            self.send_event_defer(Event::MouseScroll { x, y });
        }
    }

    // custom
    unsafe extern "C" fn draw_frame(&self, _cmd: Sel) {
        self.send_event(Event::WindowFrame { gl: None });
    }

    unsafe extern "C" fn wakeup(&self, _cmd: Sel) {
        self.send_event(Event::Wakeup);
    }

    unsafe extern "C" fn handle_notification(&self, _cmd: Sel, notif: &NSNotification) {
        unsafe {
            let Some(object) = notif.object() else { return };
            let Some(window) = self.view.window() else {
                return;
            };
            let Some(first_responder) = window.firstResponder() else {
                return;
            };

            if std::ptr::addr_eq(&*object, &*window)
                && std::ptr::addr_eq(&*first_responder, &*self.view)
            {
                self.send_event_defer(Event::WindowFocus {
                    focus: window.isKeyWindow(),
                });
            }
        }
    }

    // NSDraggingDestination
    unsafe extern "C" fn wants_periodic_dragging_updates(&self, _cmd: Sel) -> Bool {
        Bool::NO
    }

    unsafe extern "C" fn dragging_entered(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> NSDragOperation {
        NSDragOperation::empty()
    }

    unsafe extern "C" fn dragging_updated(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> NSDragOperation {
        NSDragOperation::empty()
    }

    unsafe extern "C" fn dragging_exited(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) {
    }

    unsafe extern "C" fn prepare_for_drag_operation(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn perform_drag_operation(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> Bool {
        Bool::NO
    }

    fn register_class() -> Result<&'static AnyClass, Error> {
        let class_name =
            CString::new(format!("picoview-{}", random_id())).expect("unexpected nul terminator?");

        let mut builder = match ClassBuilder::new(&class_name, NSView::class()) {
            Some(builder) => builder,
            None => return Err(Error::PlatformError("Failed to register class".to_string())),
        };

        builder.add_ivar::<*mut c_void>(c"_context");

        unsafe {
            // NSView
            builder.add_method(
                sel!(initWithFrame:),
                Self::init_with_frame as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(dealloc),
                Self::dealloc as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(viewDidChangeBackingProperties:),
                Self::view_did_change_backing_properties as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(acceptsFirstMouse:),
                Self::accepts_first_mouse as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(acceptsFirstResponder),
                Self::accepts_first_responder as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(isFlipped),
                Self::is_flipped as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(keyDown:),
                Self::key_down as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(keyUp:),
                Self::key_up as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(flagsChanged:),
                Self::flags_changed as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseMoved:),
                Self::mouse_moved as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseDragged:),
                Self::mouse_moved as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(rightMouseDragged:),
                Self::mouse_moved as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(otherMouseDragged:),
                Self::mouse_moved as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseDown:),
                Self::mouse_down as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseUp:),
                Self::mouse_up as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(rightMouseDown:),
                Self::mouse_down as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(rightMouseUp:),
                Self::mouse_up as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(otherMouseDown:),
                Self::mouse_down as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(otherMouseUp:),
                Self::mouse_up as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseExited:),
                Self::mouse_exited as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(scrollWheel:),
                Self::scroll_wheel as unsafe extern "C" fn(_, _, _) -> _,
            );

            // custom
            builder.add_method(
                sel!(picoview_drawFrame),
                Self::draw_frame as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(picoview_wakeup),
                Self::wakeup as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(picoview_handleNotification:),
                Self::handle_notification as unsafe extern "C" fn(_, _, _) -> _,
            );

            // NSDraggingDestination
            builder.add_method(
                sel!(wantsPeriodicDraggingUpdates),
                Self::wants_periodic_dragging_updates as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(draggingEntered:),
                Self::dragging_entered as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(draggingUpdated:),
                Self::dragging_updated as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(draggingExited:),
                Self::dragging_exited as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(prepareForDragOperation:),
                Self::prepare_for_drag_operation as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(performDragOperation:),
                Self::perform_drag_operation as unsafe extern "C" fn(_, _, _) -> _,
            );
        }

        Ok(builder.register())
    }
}

impl PlatformWindow for WindowView {
    fn close(&self) {
        if self.inner().is_closed.replace(true) {
            return;
        }

        unsafe {
            self.view.removeFromSuperview();
        }

        if let Some(app) = self.inner().application.take() {
            if let Some(window) = self.view.window() {
                window.close();
            }

            app.stop(Some(&app));
        }
    }

    fn waker(&self) -> WindowWaker {
        WindowWaker(self.inner().waker.clone())
    }

    fn set_title(&self, title: &str) {
        let is_blocking = self.inner().application.borrow().is_some();
        if is_blocking && let Some(window) = self.view.window() {
            window.setTitle(&NSString::from_str(title));
        }
    }

    fn set_cursor_icon(&self, cursor: MouseCursor) {
        unsafe {
            let old_cursor = self.inner().current_cursor.replace(cursor);
            if old_cursor != cursor {
                match get_cursor(cursor) {
                    Some(cursor) => {
                        if old_cursor == MouseCursor::Hidden {
                            NSCursor::unhide();
                        }

                        cursor.set();
                    }

                    None => NSCursor::hide(),
                };
            }
        }
    }

    fn set_cursor_position(&self, point: Point) {
        unsafe {
            if let Some(window) = self.view.window() {
                let main_thread = MainThreadMarker::new_unchecked();
                let window_position = self
                    .view
                    .convertPoint_toView(NSPoint::new(point.x as _, point.y as _), None);
                let screen_position = window.convertPointToScreen(window_position);
                let screen_height = NSScreen::mainScreen(main_thread)
                    .map(|screen| screen.frame().size.height)
                    .unwrap_or_default();

                CGWarpMouseCursorPosition(NSPoint::new(
                    screen_position.x as _,
                    (screen_height - screen_position.y) as _,
                ));
            }
        }
    }

    fn set_size(&self, size: Size) {
        unsafe {
            let is_blocking = self.inner().application.borrow().is_some();
            if is_blocking && let Some(window) = self.view.window() {
                window.setContentSize(CGSize {
                    width: size.width as _,
                    height: size.height as _,
                });
            }

            self.view.setFrameSize(NSSize {
                width: size.width as _,
                height: size.height as _,
            });
        }
    }

    fn set_position(&self, point: Point) {
        unsafe {
            let is_blocking = self.inner().application.borrow().is_some();
            if is_blocking && let Some(window) = self.view.window() {
                window.setFrameOrigin(CGPoint {
                    x: point.x as _,
                    y: point.y as _,
                });
            } else {
                self.view.setFrameOrigin(NSPoint {
                    x: point.x as _,
                    y: point.y as _,
                });
            }
        }
    }

    fn set_visible(&self, visible: bool) {
        let is_blocking = self.inner().application.borrow().is_some();
        if is_blocking && let Some(window) = self.view.window() {
            if visible {
                window.orderFront(None);
            } else {
                window.orderOut(None);
            }
        } else {
            self.view.setHidden(!visible);
        }
    }

    fn open_url(&self, url: &str) -> bool {
        spawn_detached(std::process::Command::new("/usr/bin/open").arg(url)).is_ok()
    }

    fn get_clipboard_text(&self) -> Option<String> {
        get_clipboard_text()
    }

    fn set_clipboard_text(&self, text: &str) -> bool {
        set_clipboard_text(text)
    }

    fn window_handle(&self) -> rwh_06::RawWindowHandle {
        unsafe {
            rwh_06::RawWindowHandle::AppKit(rwh_06::AppKitWindowHandle::new(
                NonNull::new_unchecked(&self.view as *const _ as *mut _),
            ))
        }
    }

    fn display_handle(&self) -> rwh_06::RawDisplayHandle {
        rwh_06::RawDisplayHandle::AppKit(rwh_06::AppKitDisplayHandle::new())
    }
}

impl PlatformWaker for WindowViewWaker {
    fn wakeup(&self) -> Result<(), WakeupError> {
        if let Some(view) = self.weak.load() {
            unsafe {
                view.view
                    .performSelectorOnMainThread_withObject_waitUntilDone(
                        sel!(picoview_wakeup),
                        None,
                        false,
                    );
            }

            Ok(())
        } else {
            Err(WakeupError::Disconnected)
        }
    }
}

impl Drop for WindowView {
    fn drop(&mut self) {
        // we need to drop this before OsWindowView gets dropped, see the safety comment at the handler initialization place
        self.inner().event_handler.take();
    }
}
