use super::display::*;
use crate::platform::mac::gl::GlContext;
use crate::platform::mac::util::*;
use crate::platform::{OpenMode, PlatformOpenGl, PlatformWaker, PlatformWindow};
use crate::*;
use block2::RcBlock;
use objc2::rc::{Allocated, Retained, Weak};
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
    NSDragOperation, NSDraggingInfo, NSEvent, NSEventMask, NSEventModifierFlags, NSEventType,
    NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypeString, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSViewFrameDidChangeNotification, NSWindow,
    NSWindowDidResignKeyNotification, NSWindowOrderingMode, NSWindowStyleMask,
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
use std::ops::Deref;
use std::ptr::{NonNull, null_mut};
use std::sync::Arc;

#[repr(C)]
pub struct WindowImpl {
    view: NSView,
}

pub struct WindowImplInner {
    _display_link: DisplayLink,
    key_event_monitor: Option<Retained<AnyObject>>,

    gl_context: Option<GlContext>,

    application: RefCell<Option<Retained<NSApplication>>>,
    waker: Arc<WindowWakerImpl>,

    event_queue: RefCell<VecDeque<Event<'static>>>,
    current_cursor: Cell<MouseCursor>,

    #[allow(clippy::type_complexity)]
    event_handler: RefCell<Option<Box<dyn FnMut(Event)>>>,

    is_closed: Cell<bool>,
    is_embedded: bool,
}

struct WindowWakerImpl {
    weak: Weak<WindowImpl>,
}

unsafe impl Send for WindowWakerImpl {}
unsafe impl Sync for WindowWakerImpl {}

unsafe impl Message for WindowImpl {}
unsafe impl RefEncode for WindowImpl {
    const ENCODING_REF: Encoding = NSView::ENCODING_REF;
}

impl Deref for WindowImpl {
    type Target = WindowImplInner;

    fn deref(&self) -> &Self::Target {
        self.inner().expect("WindowImplInner is not initialized")
    }
}

// rust methods stuff
impl WindowImpl {
    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<WindowWaker, WindowError> {
        let main_thread = MainThreadMarker::new()
            .ok_or_else(|| WindowError::Platform("not on main thread".into()))?;

        match mode {
            OpenMode::Blocking => unsafe {
                let app = NSApp(main_thread);
                app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

                let window = Self::create_window(&options, main_thread)?;
                let view = Self::create_view(options, Some(app.clone()), false, main_thread)?;

                window.setContentView(Some(&view.view));
                window.makeFirstResponder(Some(&view.view));
                window.setReleasedWhenClosed(true);

                // Set the window delegate to our view
                // NSWindowDelegate has no required methods, so this is safe
                window.setDelegate(Some(std::mem::transmute::<
                    &WindowImpl,
                    &objc2::runtime::ProtocolObject<dyn objc2_app_kit::NSWindowDelegate>,
                >(&*view)));

                app.run();
                Ok(WindowWaker::default())
            },

            OpenMode::Transient(parent) => unsafe {
                let parent_view = match parent {
                    rwh_06::RawWindowHandle::AppKit(window) => {
                        window.ns_view.as_ptr() as *mut NSView
                    }
                    _ => return Err(WindowError::InvalidParent),
                };

                let window = Self::create_window(&options, main_thread)?;
                let view = Self::create_view(options, None, false, main_thread)?;

                window.setContentView(Some(&view.view));
                window.makeFirstResponder(Some(&view.view));

                // Set the window delegate to our view
                // NSWindowDelegate has no required methods, so this is safe
                window.setDelegate(Some(std::mem::transmute::<
                    &WindowImpl,
                    &objc2::runtime::ProtocolObject<dyn objc2_app_kit::NSWindowDelegate>,
                >(&*view)));

                if let Some(parent_window) = (*parent_view).window() {
                    parent_window.addChildWindow_ordered(&window, NSWindowOrderingMode::Above);
                }

                Ok(view.waker())
            },

            OpenMode::Embedded(parent) => unsafe {
                let parent_view = match parent {
                    rwh_06::RawWindowHandle::AppKit(window) => {
                        window.ns_view.as_ptr() as *mut NSView
                    }
                    _ => return Err(WindowError::InvalidParent),
                };

                let view = Self::create_view(options, None, true, main_thread)?;
                (*parent_view).addSubview(&view.view);
                Ok(view.waker())
            },
        }
    }

    unsafe fn create_window(
        options: &WindowBuilder,
        main_thread: MainThreadMarker,
    ) -> Result<Retained<NSWindow>, WindowError> {
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
        is_embedded: bool,
        main_thread: MainThreadMarker,
    ) -> Result<Retained<Self>, WindowError> {
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
            let view: Allocated<WindowImpl> = msg_send![class, alloc];
            let view: Retained<WindowImpl> = msg_send![view, initWithFrame: rect];

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

            let dragged_types = NSArray::arrayWithObject(NSPasteboardTypeFileURL)
                .arrayByAddingObject(NSPasteboardTypeString);

            view.view.addTrackingArea(&tracking_area);
            view.view.registerForDraggedTypes(&dragged_types);
            view.view.setPostsFrameChangedNotifications(true);

            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &view.view,
                sel!(windowDidResignKeyNotification:),
                Some(NSWindowDidResignKeyNotification),
                None,
            );

            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &view.view,
                sel!(viewFrameDidChangeNotification:),
                Some(NSViewFrameDidChangeNotification),
                None,
            );

            view
        };

        let gl_context = if let Some(config) = options.opengl {
            GlContext::new(&view.view, config, main_thread).ok()
        } else {
            None
        };

        let display_link = {
            let view = Weak::from_retained(&view);
            DisplayLink::new(Box::new(move || {
                if let Some(view) = view.load() {
                    view.send_event(Event::WindowFrame);
                }
            }))?
        };

        // https://github.com/Tremus/CPLUG/blob/master/src/cplug_extensions/window_osx.m#L278
        let key_event_monitor = unsafe {
            let view = Weak::from_retained(&view);
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::KeyDown | NSEventMask::KeyUp,
                &RcBlock::new(move |event: NonNull<NSEvent>| {
                    let event = &*event.as_ptr();
                    let mut capture = false;

                    if let Some(view) = view.load()
                        && let Some(key) = keycode_to_key(event.keyCode())
                    {
                        if event.r#type() == NSEventType::KeyDown {
                            view.send_event(Event::KeyDown {
                                key,
                                capture: &mut capture,
                            });
                        } else {
                            view.send_event(Event::KeyUp {
                                key,
                                capture: &mut capture,
                            });
                        }
                    }

                    match capture {
                        true => null_mut(),
                        false => NonNull::from(event).as_ptr(),
                    }
                }),
            )
        };

        view.set_inner(Some(Box::new(WindowImplInner {
            _display_link: display_link,
            key_event_monitor,

            application: RefCell::new(blocking),
            gl_context,

            waker: Arc::new(WindowWakerImpl {
                weak: Weak::from_retained(&view),
            }),

            event_queue: RefCell::new(VecDeque::default()),
            event_handler: RefCell::new(None),

            current_cursor: Cell::new(MouseCursor::Default),
            is_closed: Cell::new(false),
            is_embedded,
        })));

        // SAFETY: we erase the lifetime of our OsWindowView; it should be safe to do so
        // because:
        //  - because our window instance has a stable address for the whole lifetime of
        //    the window (due to being stored as Retained)
        //  - we manually dispose of our handler before WindowImpl gets dropped (see
        //    drop impl)
        //  - we promise to not move the handler to a different thread as appkit api is
        //    expected to be single threaded (as that would violate the handler's !Send
        //    requirement)
        unsafe {
            view.event_handler
                .replace(Some((options.factory)(Window(&*Retained::as_ptr(&view)))));
        }

        Ok(view)
    }

    fn send_event(&self, event: Event) {
        if let Ok(mut handler) = self.event_handler.try_borrow_mut() {
            if let Some(handler) = handler.as_mut() {
                (handler)(event);

                while let Some(event) = self.event_queue.borrow_mut().pop_front() {
                    (handler)(event);
                }
            }
        } else {
            debug_assert!(false, "event handler reentrancy: {:?}", event);
        }
    }

    fn send_event_defer(&self, event: Event<'static>) {
        if self.event_handler.try_borrow_mut().is_ok() {
            self.send_event(event);
        } else {
            self.event_queue.borrow_mut().push_back(event);
        }
    }

    fn send_event_mouse_move(&self, position: CGPoint) {
        let absolute = point_window_to_global(position, &self.view);
        let relative = point_window_to_local(position, &self.view);

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

    fn set_inner(&self, context: Option<Box<WindowImplInner>>) {
        unsafe {
            self.view
                .class()
                .instance_variable(c"_context")
                .unwrap_unchecked()
                .load_ptr::<*mut c_void>(&self.view)
                .write(
                    context
                        .map(|x| Box::into_raw(x) as *mut c_void)
                        .unwrap_or(null_mut()),
                );
        }
    }

    fn inner(&self) -> Option<&WindowImplInner> {
        unsafe {
            let ivar = self
                .view
                .class()
                .instance_variable(c"_context")
                .unwrap_unchecked();
            let context = *ivar.load::<*mut c_void>(&self.view) as *mut WindowImplInner;
            if context.is_null() {
                None
            } else {
                Some(&*context)
            }
        }
    }

    fn own_window(&self) -> Option<Retained<NSWindow>> {
        if self.is_embedded {
            None
        } else {
            self.view.window()
        }
    }
}

// objective c class stuff
impl WindowImpl {
    // NSView
    unsafe extern "C" fn init_with_frame(&self, _cmd: Sel, rect: NSRect) -> Option<&Self> {
        unsafe { msg_send![super(self, NSView::class()), initWithFrame: rect] }
    }

    unsafe extern "C" fn dealloc(&self, _cmd: Sel) {
        unsafe {
            let class = self.view.class();

            // If we actually initialized before
            if let Some(inner) = self.inner() {
                let mut inner = Box::from_raw(inner as *const _ as *mut WindowImplInner);
                self.set_inner(None);

                // we need to drop this before WindowView gets dropped, see the safety comment
                // at the handler initialization place
                inner.event_handler.take();

                // Remove notification observers we registered earlier
                NSNotificationCenter::defaultCenter().removeObserver(&self.view);

                // Remove our key event monitor if we set one up
                if let Some(monitor) = inner.key_event_monitor.take() {
                    NSEvent::removeMonitor(&monitor);
                }
            }

            let _: () = msg_send![super(self, NSView::class()), dealloc];
            objc_disposeClassPair(class as *const _ as *mut _);
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

    unsafe extern "C" fn window_should_close(&self, _cmd: Sel, _: Option<&AnyObject>) -> Bool {
        self.send_event_defer(Event::WindowClose);
        Bool::NO
    }

    unsafe extern "C" fn accepts_first_mouse(&self, _cmd: Sel, _event: &NSEvent) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn accepts_first_responder(&self, _cmd: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn become_first_responder(&self, _cmd: Sel) -> Bool {
        self.send_event_defer(Event::WindowFocus { focus: true });
        Bool::YES
    }

    unsafe extern "C" fn resign_first_responder(&self, _cmd: Sel) -> Bool {
        self.send_event_defer(Event::WindowFocus { focus: false });
        Bool::YES
    }

    unsafe extern "C" fn is_flipped(&self, _cmd: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn flags_changed(&self, _cmd: Sel, event: &NSEvent) {
        self.send_event_defer(Event::KeyModifiers {
            modifiers: flags_to_modifiers((*event).modifierFlags()),
        });
    }

    unsafe extern "C" fn mouse_moved(&self, _cmd: Sel, event: &NSEvent) {
        self.send_event_mouse_move(event.locationInWindow());
    }

    unsafe extern "C" fn mouse_down(&self, _cmd: Sel, event: &NSEvent) {
        let button = match (*event).buttonNumber() {
            0 => Some(MouseButton::Left),
            1 => Some(MouseButton::Right),
            2 => Some(MouseButton::Middle),
            3 => Some(MouseButton::Back),
            4 => Some(MouseButton::Forward),
            _ => None,
        };

        self.send_event_mouse_move(event.locationInWindow());

        if let Some(button) = button {
            self.send_event_defer(Event::MouseDown { button });
        }

        if let Some(window) = self.view.window() {
            window.makeFirstResponder(Some(&self.view));
        }
    }

    unsafe extern "C" fn mouse_up(&self, _cmd: Sel, event: &NSEvent) {
        let button = match (*event).buttonNumber() {
            0 => Some(MouseButton::Left),
            1 => Some(MouseButton::Right),
            2 => Some(MouseButton::Middle),
            3 => Some(MouseButton::Back),
            4 => Some(MouseButton::Forward),
            _ => None,
        };

        self.send_event_mouse_move(event.locationInWindow());

        if let Some(button) = button {
            self.send_event_defer(Event::MouseUp { button });
        }
    }

    unsafe extern "C" fn mouse_exited(&self, _cmd: Sel, event: &NSEvent) {
        self.send_event_mouse_move(event.locationInWindow());
        self.send_event_defer(Event::MouseLeave);
    }

    unsafe extern "C" fn scroll_wheel(&self, _cmd: Sel, event: &NSEvent) {
        let mut x = -event.scrollingDeltaX() as f32;
        let mut y = event.scrollingDeltaY() as f32;

        if event.hasPreciseScrollingDeltas() {
            x /= 10.0;
            y /= 10.0;
        }

        self.send_event_mouse_move(event.locationInWindow());
        self.send_event_defer(Event::MouseScroll { x, y });
    }

    unsafe extern "C" fn draw_rect(&self, _cmd: Sel, rect: NSRect) {
        self.send_event_defer(Event::WindowDamage {
            x: rect.origin.x as u32,
            y: rect.origin.y as u32,
            w: rect.size.width as u32,
            h: rect.size.height as u32,
        });
    }

    unsafe extern "C" fn wakeup(&self, _cmd: Sel) {
        self.send_event_defer(Event::Wakeup);
    }

    unsafe extern "C" fn window_did_resign_key_notification(
        &self,
        _cmd: Sel,
        _notif: &NSNotification,
    ) {
        if let Some(window) = self.view.window() {
            window.makeFirstResponder(None);
        }
    }

    unsafe extern "C" fn view_frame_did_change_notification(
        &self,
        _cmd: Sel,
        _notif: &NSNotification,
    ) {
        let frame = self.view.frame();
        self.send_event_defer(Event::WindowResize {
            size: Size {
                width: frame.size.width as u32,
                height: frame.size.height as u32,
            },
        });

        if let Some(gl) = &self.gl_context {
            gl.resize(frame.size.width as u32, frame.size.height as u32);
        }
    }

    // NSDraggingDestination
    unsafe extern "C" fn wants_periodic_dragging_updates(&self, _cmd: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn dragging_entered(
        &self,
        _cmd: Sel,
        info: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> NSDragOperation {
        let data = get_pasteboard(&info.draggingPasteboard());
        let point = point_window_to_local(info.draggingLocation(), &self.view);
        self.send_event_defer(Event::DragEnter {
            data,
            point: Point {
                x: point.x as f32,
                y: point.y as f32,
            },
        });

        NSDragOperation::Generic
    }

    unsafe extern "C" fn dragging_updated(
        &self,
        _cmd: Sel,
        info: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> NSDragOperation {
        let point = point_window_to_local(info.draggingLocation(), &self.view);
        self.send_event_defer(Event::DragMove {
            point: Point {
                x: point.x as f32,
                y: point.y as f32,
            },
        });

        NSDragOperation::Generic
    }

    unsafe extern "C" fn dragging_exited(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) {
        self.send_event_defer(Event::DragLeave);
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
        info: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> Bool {
        let point = point_window_to_local(info.draggingLocation(), &self.view);
        self.send_event_defer(Event::DragMove {
            point: Point {
                x: point.x as f32,
                y: point.y as f32,
            },
        });
        self.send_event_defer(Event::DragAccept);

        Bool::YES
    }

    fn register_class() -> Result<&'static AnyClass, WindowError> {
        let class_name =
            CString::new(format!("picoview-{}", random_id())).expect("unexpected nul terminator?");

        let mut builder = match ClassBuilder::new(&class_name, NSView::class()) {
            Some(builder) => builder,
            None => {
                return Err(WindowError::Platform(
                    "Failed to register class".to_string(),
                ));
            }
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
                sel!(becomeFirstResponder),
                Self::become_first_responder as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(resignFirstResponder),
                Self::resign_first_responder as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(isFlipped),
                Self::is_flipped as unsafe extern "C" fn(_, _) -> _,
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
            builder.add_method(
                sel!(drawRect:),
                Self::draw_rect as unsafe extern "C" fn(_, _, _) -> _,
            );

            // custom
            builder.add_method(
                sel!(picoview_wakeup),
                Self::wakeup as unsafe extern "C" fn(_, _) -> _,
            );

            // NSWindowDelegate methods
            builder.add_method(
                sel!(windowShouldClose:),
                Self::window_should_close as unsafe extern "C" fn(_, _, _) -> _,
            );

            // NSNotification handlers
            builder.add_method(
                sel!(windowDidResignKeyNotification:),
                Self::window_did_resign_key_notification as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(viewFrameDidChangeNotification:),
                Self::view_frame_did_change_notification as unsafe extern "C" fn(_, _, _) -> _,
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

impl PlatformWindow for WindowImpl {
    fn close(&self) {
        if self.is_closed.replace(true) {
            return;
        }

        if let Some(window) = self.own_window() {
            window.close();
        }

        self.view.removeFromSuperview();

        if let Some(app) = self.application.take() {
            app.stop(Some(&app));

            // it is stupid that we have to send a dummy event to _actually_ stop the event
            // loop but here we are, thank you apple!!!!
            app.postEvent_atStart(&NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                NSEventType::ApplicationDefined,
                NSPoint::new(0.0, 0.0),
                NSEventModifierFlags::empty(),
                0.0,
                0,
                None,
                0,
                0,
                0,
            ).expect("Failed to create dummy event"), false);
        }
    }

    fn waker(&self) -> WindowWaker {
        WindowWaker(self.waker.clone())
    }

    fn opengl(&self) -> Option<&dyn PlatformOpenGl> {
        self.gl_context
            .as_ref()
            .map(|ctx| ctx as &dyn PlatformOpenGl)
    }

    fn set_title(&self, title: &str) {
        if let Some(window) = self.own_window() {
            window.setTitle(&NSString::from_str(title));
        }
    }

    fn set_cursor_icon(&self, cursor: MouseCursor) {
        let old_cursor = self.current_cursor.replace(cursor);
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

    fn set_cursor_position(&self, point: Point) {
        CGWarpMouseCursorPosition(point_local_to_screen(
            NSPoint::new(point.x as _, point.y as _),
            &self.view,
        ));
    }

    fn set_size(&self, size: Size) {
        if let Some(window) = self.own_window() {
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

    fn set_position(&self, point: Point) {
        if let Some(window) = self.own_window() {
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

    fn set_visible(&self, visible: bool) {
        if let Some(window) = self.own_window() {
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

    fn set_clipboard(&self, data: Exchange) -> bool {
        unsafe {
            let pasteboard: Option<Retained<NSPasteboard>> =
                msg_send![NSPasteboard::class(), generalPasteboard];

            match pasteboard {
                Some(pasteboard) => set_pasteboard(&pasteboard, data),
                None => false,
            }
        }
    }

    fn get_clipboard(&self) -> Exchange {
        unsafe {
            let pasteboard: Option<Retained<NSPasteboard>> =
                msg_send![NSPasteboard::class(), generalPasteboard];

            match pasteboard {
                Some(pasteboard) => get_pasteboard(&pasteboard),
                None => Exchange::Empty,
            }
        }
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

impl PlatformWaker for WindowWakerImpl {
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
            Err(WakeupError)
        }
    }
}
