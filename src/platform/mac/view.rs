use super::RawHandle;
use super::display::{self, CGResult, CVDisplayLink, CVTimeStamp, get_displays_with_rect};
use super::util::{cstr, get_cursor, keycode2key, random_id};
use crate::{Error, Event, EventHandler, EventResponse, Modifiers, MouseButton, MouseCursor, Options, Point, Size};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{ProtocolObject, Sel};
use objc2::{AllocAnyThread, MainThreadMarker};
use objc2::{
    ClassType, Encoding, Message, RefEncode,
    declare::ClassBuilder,
    ffi::objc_disposeClassPair,
    msg_send,
    runtime::{AnyClass, Bool},
    sel,
};
use objc2_app_kit::{
    NSCursor, NSDragOperation, NSDraggingInfo, NSEvent, NSEventModifierFlags, NSPasteboardTypeFileURL, NSScreen,
    NSTrackingArea, NSTrackingAreaOptions, NSView,
};
use objc2_foundation::{NSArray, NSInvocationOperation, NSOperationQueue, NSPoint, NSRect, NSSize};
use std::cell::RefCell;
use std::fmt;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Weak};
use std::{
    ffi::{CString, c_void},
    ops::{Deref, DerefMut},
};

#[repr(C)]
pub struct OsWindowView {
    superclass: NSView,
}

#[derive(Clone)]
pub struct OsWindowHandle(Arc<OsWindowHandleInner>);

pub enum OsWindowCommand {
    SetCursorIcon(MouseCursor),
    SetCursorPosition(Point),
    SetSize(Size),
    SetPosition(Point),
    SetVisible(bool),
    SetKeyboardInput(bool),
    Close,
}

struct OsWindowHandleInner {
    view: Retained<OsWindowView>,
    commands: Sender<OsWindowCommand>,
}

struct OsWindowViewInner {
    _class: OsWindowClass,
    _display_link: CVDisplayLink,

    event_handler: EventHandler,
    input_focus: bool,
    current_cursor: MouseCursor,
    maybe_handle: Weak<OsWindowHandleInner>,
    commands: Receiver<OsWindowCommand>,
}

unsafe impl RefEncode for OsWindowView {
    const ENCODING_REF: Encoding = NSView::ENCODING_REF;
}

unsafe impl Message for OsWindowView {}
unsafe impl Send for OsWindowHandleInner {}
unsafe impl Sync for OsWindowHandleInner {}
pub struct OsWindowClass(&'static AnyClass);

impl OsWindowView {
    pub fn open(options: Options) -> Result<OsWindowHandle, Error> {
        let class = OsWindowClass::register_class()?;

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
            let view: Allocated<OsWindowView> = msg_send![class.0, alloc];
            let view: Retained<OsWindowView> = msg_send![view, initWithFrame: rect];

            let tracking_area = NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                rect,
                NSTrackingAreaOptions::MouseEnteredAndExited
                    | NSTrackingAreaOptions::MouseMoved
                    | NSTrackingAreaOptions::ActiveAlways
                    | NSTrackingAreaOptions::InVisibleRect,
                Some(&view),
                None,
            );

            let dragged_types = NSArray::arrayWithObject(NSPasteboardTypeFileURL);
            view.addTrackingArea(&tracking_area);
            view.registerForDraggedTypes(&dragged_types);

            match options.parent {
                Some(parent) => {
                    let parent_view = &*(parent.view as *const NSView);
                    parent_view.addSubview(&view);
                }
                None => {}
            }

            view
        };

        let display = {
            let displays = get_displays_with_rect(rect)?;
            let mut cv_display_link = CVDisplayLink::create_with_active_cg_displays()?;
            cv_display_link.set_output_callback(display_link_callback, (&*view) as *const _ as *mut _)?;
            cv_display_link.set_current_display(displays[0])?;
            cv_display_link.start()?;
            cv_display_link
        };

        let (sender, receiver) = channel();
        let handle = Arc::new(OsWindowHandleInner {
            view: view.clone(),
            commands: sender,
        });

        view.set_context(Box::new(RefCell::new(OsWindowViewInner {
            _class: class,
            _display_link: display,
            event_handler: options.handler,
            input_focus: false,
            current_cursor: MouseCursor::Default,
            maybe_handle: Arc::downgrade(&handle),
            commands: receiver,
        })));

        Ok(OsWindowHandle(handle))
    }

    fn has_input_focus(&self) -> bool {
        self.with_context(|context| context.input_focus)
    }

    fn send_event(&self, event: Event) -> EventResponse {
        self.with_context(|context| {
            if let Some(handle) = context.maybe_handle.upgrade() {
                (context.event_handler)(event, crate::Window::from_inner(&OsWindowHandle(handle)))
            } else {
                EventResponse::Rejected
            }
        })
    }

    // NSView
    unsafe extern "C-unwind" fn init_with_frame(&self, _cmd: Sel, rect: NSRect) -> Option<&Self> {
        unsafe { msg_send![super(self, NSView::class()), initWithFrame: rect] }
    }

    unsafe extern "C-unwind" fn dealloc(&self, _cmd: Sel) {
        unsafe {
            let ivar = self.class().instance_variable(cstr!("_context")).unwrap_unchecked();
            let context = *ivar.load::<*mut c_void>(self) as *mut Box<RefCell<OsWindowViewInner>>;
            if !context.is_null() {
                drop(Box::from_raw(context));
            }

            let _: () = msg_send![super(self, NSView::class()), dealloc];
        }
    }

    unsafe extern "C-unwind" fn accepts_first_mouse(&self, _cmd: Sel, _event: *const NSEvent) -> Bool {
        Bool::YES
    }

    unsafe extern "C-unwind" fn accepts_first_responder(&self, _cmd: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C-unwind" fn is_flipped(&self, _cmd: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C-unwind" fn key_down(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            if event.is_null() {
                return;
            }

            // autoreleasepool(|pool| {
            //     if let Some(chars) = (*event).characters() {
            //         self.send_event(Event::KeyChar(chars.as_str(pool)));
            //     }
            // });

            if let Some(key) = keycode2key((*event).keyCode()) {
                self.send_event(Event::KeyDown { key });
            }

            if !self.has_input_focus() {
                msg_send![super(self, NSView::class()), keyDown: event]
            }
        }
    }

    unsafe extern "C-unwind" fn key_up(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            if event.is_null() {
                return;
            }

            if let Some(key) = keycode2key((*event).keyCode()) {
                self.send_event(Event::KeyUp { key });
            }

            if !self.has_input_focus() {
                msg_send![super(self, NSView::class()), keyUp: event]
            }
        }
    }

    unsafe extern "C-unwind" fn flags_changed(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            const MODMAP: &[(NSEventModifierFlags, Modifiers)] = &[
                (NSEventModifierFlags::CapsLock, Modifiers::CAPS_LOCK),
                (NSEventModifierFlags::Command, Modifiers::META),
                (NSEventModifierFlags::Control, Modifiers::CTRL),
                (NSEventModifierFlags::Option, Modifiers::ALT),
                (NSEventModifierFlags::Shift, Modifiers::SHIFT),
            ];

            let flags = (*event).modifierFlags();
            let mut modifiers = Modifiers::empty();
            for (flag, modifier) in MODMAP {
                if flags.contains(*flag) {
                    modifiers.insert(*modifier);
                }
            }
            self.send_event(Event::KeyModifiers { modifiers });
        }
    }

    unsafe extern "C-unwind" fn mouse_moved(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            let point = (*event).locationInWindow();
            let point = self.convertPoint_fromView(point, None);
            self.send_event(Event::MouseMove {
                cursor: Some(Point {
                    x: point.x as _,
                    y: point.y as _,
                }),
            });
        }
    }

    unsafe extern "C-unwind" fn mouse_down(&self, _cmd: Sel, event: *const NSEvent) {
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
                self.send_event(Event::MouseDown { button });
            }
        }
    }

    unsafe extern "C-unwind" fn mouse_up(&self, _cmd: Sel, event: *const NSEvent) {
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
                self.send_event(Event::MouseUp { button });
            }
        }
    }

    unsafe extern "C-unwind" fn mouse_exited(&self, _cmd: Sel, _event: *const NSEvent) {
        self.send_event(Event::MouseMove { cursor: None });
    }

    unsafe extern "C-unwind" fn scroll_wheel(&self, _cmd: Sel, event: *const NSEvent) {
        unsafe {
            if event.is_null() {
                return;
            }

            let x = (*event).deltaX() as f32;
            let y = (*event).deltaY() as f32;

            self.send_event(Event::MouseScroll { x, y });
        }
    }

    // custom
    unsafe extern "C-unwind" fn draw_frame(&self, _cmd: Sel) {
        self.send_event(Event::WindowFrame { gl: None });
    }

    unsafe extern "C-unwind" fn pull_commands(&self, _cmd: Sel) {
        unsafe {
            self.with_context(|context| {
                while let Ok(command) = context.commands.try_recv() {
                    match command {
                        OsWindowCommand::SetCursorIcon(cursor) => {
                            if context.current_cursor != cursor {
                                match get_cursor(cursor) {
                                    Some(cursor) => {
                                        if context.current_cursor == MouseCursor::Hidden {
                                            NSCursor::unhide();
                                        }

                                        cursor.set();
                                    }

                                    None => NSCursor::hide(),
                                };

                                context.current_cursor = cursor;
                            }
                        }
                        OsWindowCommand::SetCursorPosition(point) => {
                            if let Some(window) = self.window() {
                                let window_position =
                                    self.convertPoint_toView(NSPoint::new(point.x as _, point.y as _), None);
                                let screen_position = window.convertPointToScreen(window_position);
                                let screen_height = NSScreen::mainScreen(MainThreadMarker::new_unchecked())
                                    .map(|screen| screen.frame().size.height)
                                    .unwrap_or_default();

                                display::warp_mouse_cursor_position(NSPoint::new(
                                    screen_position.x as _,
                                    (screen_height - screen_position.y) as _,
                                ));
                            }
                        }
                        OsWindowCommand::SetSize(size) => {
                            self.setFrameSize(NSSize {
                                width: size.width as _,
                                height: size.height as _,
                            });
                        }
                        OsWindowCommand::SetPosition(point) => {
                            self.setFrameOrigin(NSPoint {
                                x: point.x as _,
                                y: point.y as _,
                            });
                        }
                        OsWindowCommand::SetVisible(visible) => {
                            if let Some(window) = self.window() {
                                if visible {
                                    window.orderFront(None);
                                } else {
                                    window.orderOut(None);
                                }
                            }
                        }
                        OsWindowCommand::SetKeyboardInput(focus) => {
                            context.input_focus = focus;
                        }
                        OsWindowCommand::Close => {
                            if let Some(window) = self.window() {
                                window.close();
                            }
                        }
                    }
                }
            });
        }
    }

    // NSDraggingDestination
    unsafe extern "C-unwind" fn wants_periodic_dragging_updates(&self, _cmd: Sel) -> Bool {
        Bool::NO
    }

    unsafe extern "C-unwind" fn dragging_entered(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> NSDragOperation {
        NSDragOperation::empty()
    }

    unsafe extern "C-unwind" fn dragging_updated(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> NSDragOperation {
        NSDragOperation::empty()
    }

    unsafe extern "C-unwind" fn dragging_exited(&self, _cmd: Sel, _sender: &ProtocolObject<dyn NSDraggingInfo>) {}

    unsafe extern "C-unwind" fn prepare_for_drag_operation(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> Bool {
        Bool::YES
    }

    unsafe extern "C-unwind" fn perform_drag_operation(
        &self,
        _cmd: Sel,
        _sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> Bool {
        Bool::NO
    }

    fn set_context(&self, context: Box<RefCell<OsWindowViewInner>>) {
        unsafe {
            self.class()
                .instance_variable(cstr!("_context"))
                .unwrap_unchecked()
                .load_ptr::<*mut c_void>(self)
                .write(Box::into_raw(context) as *mut c_void);
        }
    }

    fn with_context<T>(&self, f: impl FnOnce(&mut OsWindowViewInner) -> T) -> T {
        unsafe {
            let ivar = self.class().instance_variable(cstr!("_context")).unwrap_unchecked();
            let context = *ivar.load::<*mut c_void>(self) as *mut Box<RefCell<OsWindowViewInner>>;
            let data = &*context;
            f(&mut *data.borrow_mut())
        }
    }
}

impl OsWindowClass {
    fn register_class() -> Result<OsWindowClass, Error> {
        let class_name = CString::new(format!("picoview-{}", random_id())).unwrap();

        let mut builder = match ClassBuilder::new(&class_name, NSView::class()) {
            Some(builder) => builder,
            None => return Err(Error::PlatformError("Failed to register class".to_string())),
        };

        builder.add_ivar::<*mut c_void>(cstr!("_context"));

        unsafe {
            // NSView
            builder.add_method(
                sel!(initWithFrame:),
                OsWindowView::init_with_frame as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(dealloc),
                OsWindowView::dealloc as unsafe extern "C-unwind" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(acceptsFirstMouse:),
                OsWindowView::accepts_first_mouse as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(acceptsFirstResponder),
                OsWindowView::accepts_first_responder as unsafe extern "C-unwind" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(isFlipped),
                OsWindowView::is_flipped as unsafe extern "C-unwind" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(keyDown:),
                OsWindowView::key_down as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(keyUp:),
                OsWindowView::key_up as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(flagsChanged:),
                OsWindowView::flags_changed as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseMoved:),
                OsWindowView::mouse_moved as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseDragged:),
                OsWindowView::mouse_moved as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(rightMouseDragged:),
                OsWindowView::mouse_moved as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(otherMouseDragged:),
                OsWindowView::mouse_moved as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseDown:),
                OsWindowView::mouse_down as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseUp:),
                OsWindowView::mouse_up as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(rightMouseDown:),
                OsWindowView::mouse_down as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(rightMouseUp:),
                OsWindowView::mouse_up as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(otherMouseDown:),
                OsWindowView::mouse_down as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(otherMouseUp:),
                OsWindowView::mouse_up as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(mouseExited:),
                OsWindowView::mouse_exited as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(scrollWheel:),
                OsWindowView::scroll_wheel as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );

            // custom
            builder.add_method(
                sel!(picoview_drawFrame),
                OsWindowView::draw_frame as unsafe extern "C-unwind" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(picoview_pullCommands),
                OsWindowView::pull_commands as unsafe extern "C-unwind" fn(_, _) -> _,
            );

            // NSDraggingDestination
            builder.add_method(
                sel!(wantsPeriodicDraggingUpdates),
                OsWindowView::wants_periodic_dragging_updates as unsafe extern "C-unwind" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(draggingEntered:),
                OsWindowView::dragging_entered as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(draggingUpdated:),
                OsWindowView::dragging_updated as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(draggingExited:),
                OsWindowView::dragging_exited as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(prepareForDragOperation:),
                OsWindowView::prepare_for_drag_operation as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(performDragOperation:),
                OsWindowView::perform_drag_operation as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
        }

        Ok(OsWindowClass(builder.register()))
    }
}

impl OsWindowHandle {
    pub fn raw_handle(&self) -> RawHandle {
        RawHandle {
            view: (&*self.0.view) as *const _ as *mut _,
        }
    }

    pub fn post(&self, command: OsWindowCommand) {
        unsafe {
            if self.0.commands.send(command).is_ok() {
                NSInvocationOperation::initWithTarget_selector_object(
                    NSInvocationOperation::alloc(),
                    &self.0.view,
                    sel!(picoview_pullCommands),
                    None,
                )
                .map(|operation| NSOperationQueue::mainQueue().addOperation(&operation));
            }
        }
    }
}

impl Deref for OsWindowView {
    type Target = NSView;

    fn deref(&self) -> &Self::Target {
        &self.superclass
    }
}

impl DerefMut for OsWindowView {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.superclass
    }
}

impl Drop for OsWindowClass {
    fn drop(&mut self) {
        unsafe { objc_disposeClassPair(self.0 as *const _ as _) };
    }
}

impl fmt::Debug for OsWindowHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OsWindowHandle").finish()
    }
}

unsafe extern "C" fn display_link_callback(
    _display_link: CVDisplayLink,
    _in_now: *mut CVTimeStamp,
    _in_output_time: *mut CVTimeStamp,
    _flags_in: u64,
    _flags_out: *mut u64,
    display_link_context: *mut c_void,
) -> CGResult {
    unsafe {
        let view = display_link_context as *const OsWindowView;

        NSInvocationOperation::initWithTarget_selector_object(
            NSInvocationOperation::alloc(),
            &*view,
            sel!(picoview_drawFrame),
            None,
        )
        .map(|operation| NSOperationQueue::mainQueue().addOperation(&operation));

        0
    }
}
