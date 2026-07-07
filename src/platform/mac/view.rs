use super::display::*;
use crate::platform::mac::gl::GlContext;
use crate::platform::mac::util::*;
use crate::platform::{OpenMode, PlatformOpenGl, PlatformWaker, PlatformWindow};
use crate::*;
use block2::RcBlock;
use objc2::declare::ClassBuilder;
use objc2::ffi::objc_disposeClassPair;
use objc2::rc::{Allocated, Retained, Weak};
use objc2::runtime::{AnyClass, AnyObject, Bool, ProtocolObject, Sel};
use objc2::{
    AllocAnyThread, ClassType, Encoding, MainThreadMarker, MainThreadOnly, Message, ProtocolType,
    RefEncode, msg_send, sel,
};
use objc2_app_kit::{
    NSApp, NSApplication, NSApplicationActivationPolicy, NSAutoresizingMaskOptions,
    NSBackingStoreType, NSCursor, NSDragOperation, NSDraggingInfo, NSEvent, NSEventMask,
    NSEventModifierFlags, NSEventType, NSPasteboard, NSPasteboardTypeFileURL,
    NSPasteboardTypeString, NSTrackingArea, NSTrackingAreaOptions, NSView,
    NSViewFrameDidChangeNotification, NSWindow, NSWindowDelegate,
    NSWindowDidChangeOcclusionStateNotification, NSWindowDidResignKeyNotification,
    NSWindowOcclusionState, NSWindowOrderingMode, NSWindowStyleMask,
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
use std::ptr::{NonNull, null, null_mut};
use std::sync::Arc;

const STYLE_MASK_NORMAL: NSWindowStyleMask = NSWindowStyleMask::Titled
    .union(NSWindowStyleMask::Closable)
    .union(NSWindowStyleMask::Miniaturizable)
    .union(NSWindowStyleMask::Resizable);

#[repr(C)]
pub struct WindowImpl {
    view: NSView,
}

pub struct WindowImplInner {
    _display_link: DisplayLink,
    key_event_monitor: Option<Retained<AnyObject>>,
    application: RefCell<Option<Retained<NSApplication>>>,

    gl_context: Result<GlContext, OpenGlError>,
    waker: Arc<WindowWakerImpl>,

    #[allow(clippy::type_complexity)]
    event_deferred: RefCell<VecDeque<Box<dyn FnOnce(&WindowImpl, &mut dyn WindowHandler)>>>,
    event_handler: RefCell<Option<Box<dyn WindowHandler>>>,

    last_cursor_icon: Cell<MouseCursor>,
    last_window_size: Cell<Size>,
    last_view_hidden: Cell<bool>,

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

                let window = Self::create_window(main_thread)?;
                let view = Self::create_view(&options, Some(app.clone()), false, main_thread)?;

                window.setContentView(Some(&view.view));
                window.makeFirstResponder(Some(&view.view));
                window.setReleasedWhenClosed(true);
                window.setDelegate(Some(view.as_ns_window_delegate()));

                WindowImpl::init_handler(&view, options.factory)?;

                app.run();
                Ok(WindowWaker::default())
            },

            OpenMode::Transient(parent) => unsafe {
                let parent_view = match parent {
                    rwh_06::RawWindowHandle::AppKit(window) => {
                        &*(window.ns_view.as_ptr() as *mut NSView)
                    }
                    _ => return Err(WindowError::InvalidParent),
                };

                let window = Self::create_window(main_thread)?;
                let view = Self::create_view(&options, None, false, main_thread)?;

                window.setContentView(Some(&view.view));
                window.makeFirstResponder(Some(&view.view));
                window.setDelegate(Some(view.as_ns_window_delegate()));

                WindowImpl::init_handler(&view, options.factory)?;

                if let Some(parent_window) = parent_view.window() {
                    parent_window.addChildWindow_ordered(&window, NSWindowOrderingMode::Above);
                }

                Ok(view.waker())
            },

            OpenMode::Embedded(parent) => unsafe {
                let parent_view = match parent {
                    rwh_06::RawWindowHandle::AppKit(window) => {
                        &*(window.ns_view.as_ptr() as *mut NSView)
                    }
                    _ => return Err(WindowError::InvalidParent),
                };

                let view = Self::create_view(&options, None, true, main_thread)?;
                WindowImpl::init_handler(&view, options.factory)?;
                parent_view.addSubview(&view.view);

                Ok(view.waker())
            },
        }
    }

    unsafe fn create_window(
        main_thread: MainThreadMarker,
    ) -> Result<Retained<NSWindow>, WindowError> {
        unsafe {
            let window = NSWindow::alloc(main_thread);
            let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                window,
                NSRect::new(CGPoint::default(), NSSize::new(1.0, 1.0)),
                STYLE_MASK_NORMAL,
                NSBackingStoreType::Buffered,
                false,
            );

            Ok(window)
        }
    }

    unsafe fn create_view(
        options: &WindowBuilder,
        blocking: Option<Retained<NSApplication>>,
        is_embedded: bool,
        main_thread: MainThreadMarker,
    ) -> Result<Retained<Self>, WindowError> {
        let class = Self::register_class()?;
        let view = unsafe {
            let view: Allocated<WindowImpl> = msg_send![class, alloc];
            let view: Retained<WindowImpl> = msg_send![view, initWithFrame: NSRect::default()];

            let tracking_area = NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::default(),
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
            view.view
                .setAutoresizingMask(NSAutoresizingMaskOptions::empty());
            view.view.setAutoresizesSubviews(false);

            if is_embedded {
                NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                    &view.view,
                    sel!(windowDidResignKey:),
                    Some(NSWindowDidResignKeyNotification),
                    None,
                );

                NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                    &view.view,
                    sel!(windowDidChangeOcclusionState:),
                    Some(NSWindowDidChangeOcclusionStateNotification),
                    None,
                );
            }

            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &view.view,
                sel!(viewFrameDidChange:),
                Some(NSViewFrameDidChangeNotification),
                None,
            );

            view
        };

        // opengl context if requested
        let gl_context = options
            .opengl
            .map(|opts| GlContext::new(&view.view, opts, main_thread))
            .unwrap_or_else(|| Err(OpenGlError("OpenGL not requested".to_string())));

        // vsync synced [`WindowFrame`] events
        let display_link = {
            let view = Weak::from_retained(&view);
            DisplayLink::new(Box::new(move || {
                if let Some(view) = view.load() {
                    view.non_reentrant_event(|e| e.frame());
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

                    let Some(view) = view.load() else {
                        return NonNull::from(event).as_ptr();
                    };

                    let Some(key) = keycode_to_key(event.keyCode()) else {
                        return NonNull::from(event).as_ptr();
                    };

                    let is_down = event.r#type() == NSEventType::KeyDown;
                    let capture = view
                        .non_reentrant_event(|e| e.key_press(key, is_down))
                        .unwrap_or(false);

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

            event_deferred: RefCell::new(VecDeque::new()),
            event_handler: RefCell::new(None),

            last_cursor_icon: Cell::new(MouseCursor::Default),
            last_window_size: Cell::new(Size::default()),
            last_view_hidden: Cell::new(false),

            is_closed: Cell::new(false),
            is_embedded,
        })));

        Ok(view)
    }

    fn init_handler(this: &Retained<Self>, factory: WindowFactory) -> Result<(), WindowError> {
        // SAFETY: we erase the lifetime of our WindowImpl; it should be safe to do so
        // because:
        //  - because our window instance has a stable address for the whole lifetime of
        //    the window (due to being stored as Retained)
        //  - we manually dispose of our handler before WindowImpl gets dropped (see
        //    drop impl)
        //  - we promise to not move the handler to a different thread as appkit api is
        //    expected to be single threaded (as that would violate the handler's !Send
        //    requirement)
        let handler = unsafe {
            match factory(Window(&*Retained::as_ptr(this))) {
                Ok(handler) => handler,
                Err(error) => return Err(WindowError::Factory(error)),
            }
        };

        this.event_handler.replace(Some(handler));
        Ok(())
    }

    /// Run a closure with exclusive access to the window's event handler.
    ///
    /// Panics if [`Self::non_reentrant_event`] is called inside of another
    /// [`Self::non_reentrant_event`]. To safely post a task, use
    /// [`Self::post_deferred`].
    fn non_reentrant_event<R>(&self, call: impl FnOnce(&mut dyn WindowHandler) -> R) -> Option<R> {
        let mut handler = self
            .event_handler
            .try_borrow_mut()
            .expect("unhandled callback reentrancy");

        // handler might be None if the window is being dropped, in which case we return
        // None
        if let Some(handler) = handler.as_mut() {
            let result = Some(call(&mut **handler));

            loop {
                // event_queue must NOT be borrowed while calling the handler, so we have to
                // reborrow it every time
                let Some(event) = self.event_deferred.borrow_mut().pop_front() else {
                    break;
                };

                event(self, &mut **handler);
            }

            result
        } else {
            None
        }
    }

    /// Run a closure with exclusive access to the window's event handler.
    ///
    /// Unlike [`Self::non_reentrant_event`], this function will not panic if
    /// called inside of another [`Self::non_reentrant_event`]. Instead, the
    /// closure will be deferred and run later.
    ///
    /// For that reason it cannot return a value, and the closure must be
    /// `'static`.
    fn deferred_event(&self, task: impl FnOnce(&Self, &mut dyn WindowHandler) + 'static) {
        if self.event_handler.try_borrow_mut().is_ok() {
            self.non_reentrant_event(|handler| task(self, handler));
        } else {
            self.event_deferred.borrow_mut().push_back(Box::new(task));
        }
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

    fn convert_point_to_picoview(&self, point: NSPoint) -> Point {
        let backing = self.view.convertPointToBacking(NSPoint {
            x: point.x,
            y: point.y - self.view.frame().size.height,
        });

        Point {
            x: backing.x,
            y: backing.y,
        }
    }

    fn as_ns_window_delegate(&self) -> &ProtocolObject<dyn NSWindowDelegate> {
        // SAFETY: this is safe, this is the same thing as [`ProtocolObject::from_ref`],
        // and we ensure that we implement the NSWindowDelegate protocol (see
        // [`Self::register_class`])
        unsafe { std::mem::transmute::<&Self, &ProtocolObject<dyn NSWindowDelegate>>(self) }
    }
}

// objective c class stuff
impl WindowImpl {
    // NSView
    unsafe extern "C" fn init_with_frame(&self, _: Sel, rect: NSRect) -> Option<&Self> {
        unsafe { msg_send![super(self, NSView::class()), initWithFrame: rect] }
    }

    unsafe extern "C" fn dealloc(&self, _: Sel) {
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
        // keep physical size
        self.set_size(self.last_window_size.get());

        // let the handler handle it now
        self.deferred_event(|this, e| e.scale_changed(this.scale()));
    }

    unsafe extern "C" fn window_should_close(&self, _: Sel, _: Option<&AnyObject>) -> Bool {
        self.deferred_event(|_, e| e.close_requested());
        Bool::NO
    }

    unsafe extern "C" fn window_did_move(&self, _: Sel, _: Option<&AnyObject>) {
        if let Some(window) = self.own_window() {
            let position = window.frame().origin;
            let position = Point {
                x: position.x,
                y: position.y,
            };

            self.deferred_event(move |_, e| e.position_changed(position));
        }
    }

    unsafe extern "C" fn window_did_resign_key(&self, _: Sel, _notif: &NSNotification) {
        if let Some(window) = self.view.window() {
            window.makeFirstResponder(None);
        }
    }

    unsafe extern "C" fn window_did_change_occlusion_state(&self, sel: Sel, _: &NSNotification) {
        if self.last_view_hidden.get() {
            return;
        }

        unsafe {
            self.view_did_unhide(sel);
        }
    }

    unsafe extern "C" fn view_did_hide(&self, _: Sel) {
        self.last_view_hidden.set(true);
        self.deferred_event(|_, e| e.visibility_changed(WindowVisibility::Hidden));
    }

    unsafe extern "C" fn view_did_unhide(&self, _: Sel) {
        self.last_view_hidden.set(false);

        if let Some(window) = self.view.window() {
            let visibility = if window
                .occlusionState()
                .contains(NSWindowOcclusionState::Visible)
            {
                WindowVisibility::Normal
            } else if window.isMiniaturized() {
                WindowVisibility::Minimized
            } else {
                WindowVisibility::Occluded
            };

            self.deferred_event(move |_, e| e.visibility_changed(visibility));
        }
    }

    unsafe extern "C" fn view_frame_did_change_notification(
        &self,
        _: Sel,
        _notif: &NSNotification,
    ) {
        let logical = self.view.frame();
        let backing = self.view.convertRectToBacking(logical);
        let size = Size {
            width: backing.size.width as u32,
            height: backing.size.height as u32,
        };

        if self.last_window_size.replace(size) == size {
            return;
        }

        if let Ok(gl) = &self.gl_context {
            gl.resize(logical.size.width, logical.size.height);
        }

        self.deferred_event(|this, e| e.size_changed(this.last_window_size.get()));
    }

    unsafe extern "C" fn accepts_first_mouse(&self, _: Sel, _event: &NSEvent) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn accepts_first_responder(&self, _: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn become_first_responder(&self, _: Sel) -> Bool {
        self.deferred_event(|_, e| e.focus_changed(true));
        Bool::YES
    }

    unsafe extern "C" fn resign_first_responder(&self, _: Sel) -> Bool {
        self.deferred_event(|_, e| e.focus_changed(false));
        Bool::YES
    }

    unsafe extern "C" fn is_flipped(&self, _: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn flags_changed(&self, _: Sel, event: &NSEvent) {
        let modifiers = flags_to_modifiers((*event).modifierFlags());
        self.deferred_event(move |_, e| e.key_modifiers(modifiers));
    }

    unsafe extern "C" fn mouse_moved(&self, _: Sel, event: &NSEvent) {
        let point = self.convert_point_to_picoview(event.locationInWindow());
        self.deferred_event(move |_, e| e.mouse_move(point));
    }

    unsafe extern "C" fn mouse_button(&self, _: Sel, event: &NSEvent) {
        let is_down = event.r#type() == NSEventType::LeftMouseDown
            || event.r#type() == NSEventType::RightMouseDown
            || event.r#type() == NSEventType::OtherMouseDown;

        let button = match event.buttonNumber() {
            0 => MouseButton::Left,
            1 => MouseButton::Right,
            2 => MouseButton::Middle,
            3 => MouseButton::Back,
            4 => MouseButton::Forward,
            _ => return,
        };

        if is_down && let Some(window) = self.view.window() {
            window.makeFirstResponder(Some(&self.view));
        }

        let point = self.convert_point_to_picoview(event.locationInWindow());
        self.deferred_event(move |_, e| {
            e.mouse_move(point);
            e.mouse_press(button, is_down);
        });
    }

    unsafe extern "C" fn mouse_exited(&self, _: Sel, _event: &NSEvent) {
        self.deferred_event(|_, e| e.mouse_leave());
        self.set_cursor_icon(MouseCursor::Default);
    }

    unsafe extern "C" fn scroll_wheel(&self, _: Sel, event: &NSEvent) {
        let mut x = -event.scrollingDeltaX();
        let mut y = event.scrollingDeltaY();

        if event.hasPreciseScrollingDeltas() {
            x /= 10.0;
            y /= 10.0;
        }

        let point = self.convert_point_to_picoview(event.locationInWindow());
        self.deferred_event(move |_, e| {
            e.mouse_move(point);
            e.mouse_scroll(x, y);
        });
    }

    unsafe extern "C" fn magnify_with_event(&self, _: Sel, event: &NSEvent) {
        let delta = event.magnification();
        self.deferred_event(move |_, e| e.gesture_zoom(delta));
    }

    unsafe extern "C" fn rotate_with_event(&self, _: Sel, event: &NSEvent) {
        let delta = event.rotation() as f64;
        self.deferred_event(move |_, e| e.gesture_rotate(delta));
    }

    unsafe extern "C" fn draw_rect(&self, _: Sel, _: NSRect) {
        let mut buffer = [null(); 64];
        let mut count = 0;

        unsafe {
            self.view
                .getRectsBeingDrawn_count(buffer.as_mut_ptr(), &mut count);

            for rect in buffer.into_iter().take(count as usize) {
                if rect.is_null() {
                    continue;
                }

                let rect = *rect;
                let rect = Rect::xywh(
                    rect.origin.x.floor() as i32,
                    rect.origin.y.floor() as i32,
                    rect.size.width.ceil() as u32,
                    rect.size.height.ceil() as u32,
                );

                self.deferred_event(move |_, e| e.damage(rect));
            }
        }
    }

    unsafe extern "C" fn wakeup(&self, _: Sel) {
        self.deferred_event(|_, e| e.wakeup());
    }

    // NSDraggingDestination
    unsafe extern "C" fn wants_periodic_dragging_updates(&self, _: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn dragging_entered(
        &self,
        _: Sel,
        info: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> NSDragOperation {
        let data = get_pasteboard(&info.draggingPasteboard());
        let point = self.convert_point_to_picoview(info.draggingLocation());
        let effect = self
            .non_reentrant_event(|e| e.drag_enter(data, point))
            .unwrap_or(DropEffect::Reject);

        encode_drop_effect(effect)
    }

    unsafe extern "C" fn dragging_updated(
        &self,
        _: Sel,
        info: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> NSDragOperation {
        let point = self.convert_point_to_picoview(info.draggingLocation());
        let effect = self
            .non_reentrant_event(|e| e.drag_move(point))
            .unwrap_or(DropEffect::Reject);

        encode_drop_effect(effect)
    }

    unsafe extern "C" fn dragging_exited(
        &self,
        _: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) {
        self.deferred_event(|_, e| e.drag_leave());
    }

    unsafe extern "C" fn prepare_for_drag_operation(
        &self,
        _: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn perform_drag_operation(
        &self,
        _: Sel,
        info: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> Bool {
        let point = self.convert_point_to_picoview(info.draggingLocation());
        let accept = self
            .non_reentrant_event(|e| {
                if e.drag_move(point) == DropEffect::Reject {
                    return false;
                }

                if e.drag_accept() == DropEffect::Reject {
                    return false;
                }

                true
            })
            .unwrap_or(false);

        accept.into()
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
                Self::mouse_button as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseUp:),
                Self::mouse_button as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(rightMouseDown:),
                Self::mouse_button as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(rightMouseUp:),
                Self::mouse_button as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(otherMouseDown:),
                Self::mouse_button as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(otherMouseUp:),
                Self::mouse_button as unsafe extern "C" fn(_, _, _) -> _,
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
                sel!(magnifyWithEvent:),
                Self::magnify_with_event as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(rotateWithEvent:),
                Self::rotate_with_event as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(drawRect:),
                Self::draw_rect as unsafe extern "C" fn(_, _, _) -> _,
            );

            builder.add_method(
                sel!(viewFrameDidChange:),
                Self::view_frame_did_change_notification as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(viewDidHide),
                Self::view_did_hide as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(viewDidUnhide),
                Self::view_did_unhide as unsafe extern "C" fn(_, _) -> _,
            );

            // custom
            builder.add_method(
                sel!(picoview_wakeup),
                Self::wakeup as unsafe extern "C" fn(_, _) -> _,
            );

            // NSWindowDelegate methods & NSNotification handlers
            builder.add_method(
                sel!(windowShouldClose:),
                Self::window_should_close as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(windowDidResize:),
                Self::window_did_move as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(windowDidMove:),
                Self::window_did_move as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(windowDidResignKey:),
                Self::window_did_resign_key as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(windowDidChangeOcclusionState:),
                Self::window_did_change_occlusion_state as unsafe extern "C" fn(_, _, _) -> _,
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

            builder.add_protocol(
                <dyn objc2_app_kit::NSWindowDelegate>::protocol()
                    .expect("unknown protocol: NSWindowDelegate"),
            );
            builder.add_protocol(
                <dyn objc2_app_kit::NSDraggingDestination>::protocol()
                    .expect("unknown protocol: NSDraggingDestination"),
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
            window.setDelegate(None);
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

    fn opengl(&self) -> Result<&dyn PlatformOpenGl, OpenGlError> {
        match &self.gl_context {
            Ok(gl) => Ok(gl),
            Err(err) => Err(err.clone()),
        }
    }

    fn set_title(&self, title: &str) {
        if let Some(window) = self.own_window() {
            window.setTitle(&NSString::from_str(title));
        }
    }

    fn set_decorations(&self, decorations: bool) {
        if let Some(window) = self.own_window() {
            let mut style = window.styleMask();

            if decorations {
                style.insert(STYLE_MASK_NORMAL);
            } else {
                style.remove(STYLE_MASK_NORMAL);
            }

            window.setStyleMask(style);
        }
    }

    fn set_cursor_icon(&self, cursor: MouseCursor) {
        let old_cursor = self.last_cursor_icon.replace(cursor);
        if old_cursor != cursor {
            if old_cursor == MouseCursor::Hidden {
                NSCursor::unhide();
            }

            if cursor == MouseCursor::Hidden {
                NSCursor::hide();
            } else {
                best_cursor_icon_for(cursor).set();
            }
        }
    }

    fn set_cursor_position(&self, point: Point) {
        let point = self
            .view
            .convertPointFromBacking(NSPoint::new(point.x as _, point.y as _));
        let point = self.view.convertPoint_toView(point, None);
        let point = self.view.window().map(|w| w.convertPointToScreen(point));

        if let Some(point) = point {
            CGWarpMouseCursorPosition(point);
        }
    }

    fn set_size(&self, size: Size) {
        if self.last_window_size.get() == size {
            return;
        }

        let size = self.view.convertSizeFromBacking(CGSize {
            width: size.width as f64,
            height: size.height as f64,
        });

        if let Ok(gl) = &self.gl_context {
            gl.resize(size.width, size.height);
        }

        if let Some(window) = self.own_window() {
            window.setContentSize(size);
        }

        self.view.setFrameSize(size);
    }

    fn set_min_size(&self, size: Size) {
        if let Some(window) = self.own_window() {
            window.setMinSize(CGSize {
                width: size.width as _,
                height: size.height as _,
            });
        }
    }

    fn set_max_size(&self, size: Size) {
        if let Some(window) = self.own_window() {
            window.setMaxSize(CGSize {
                width: size.width as _,
                height: size.height as _,
            });
        }
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
                window.makeKeyAndOrderFront(None);
            } else {
                window.orderOut(None);
            }
        }

        self.view.setHidden(!visible);
    }

    fn scale(&self) -> f64 {
        self.view
            .window()
            .map(|w| w.backingScaleFactor())
            .unwrap_or(1.0)
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
