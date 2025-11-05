use super::{
    util::{load_function_dynamic, wait_vsync},
    window::WM_USER_FRAME_PACER,
};
use crate::{
    Error, MouseCursor,
    platform::win::{
        util::hinstance,
        window::{WM_USER_KEY_DOWN, WM_USER_KEY_UP, WindowImpl},
    },
};
use std::{
    mem::zeroed,
    ptr::null_mut,
    sync::{
        Arc, Mutex, Weak,
        mpsc::{Receiver, Sender, TryRecvError, channel},
    },
    thread::spawn,
};
use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            CallNextHookEx, HC_ACTION, HCURSOR, HHOOK, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_HELP,
            IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
            IDC_WAIT, LoadCursorW, MSG, PM_REMOVE, SendMessageW, SetWindowsHookExW,
            USER_DEFAULT_SCREEN_DPI, UnhookWindowsHookEx, WH_GETMESSAGE, WM_KEYDOWN, WM_KEYUP,
            WM_USER,
        },
    },
    core::PCWSTR,
};

unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

pub struct Connection {
    cursor_cache: CursorCache,
    keyboard_hook: HHOOK,

    loop_sender: Sender<(usize, bool)>,

    dl_set_thread_dpi_awareness_context: Option<unsafe fn(usize) -> usize>,
    dl_get_dpi_for_window: Option<unsafe fn(HWND) -> u32>,
}

impl Connection {
    pub fn get() -> Result<Arc<Self>, Error> {
        static INSTANCE: Mutex<Weak<Connection>> = Mutex::new(Weak::new());

        let mut lock = INSTANCE.lock().unwrap();
        if let Some(conn) = lock.upgrade() {
            return Ok(conn);
        }

        let conn = Self::create()?;
        *lock = Arc::downgrade(&conn);
        Ok(conn)
    }

    pub fn load_cursor(&self, cursor: MouseCursor) -> HCURSOR {
        self.cursor_cache.get(cursor)
    }

    pub fn try_set_thread_dpi_awareness_monitor_aware(&self) -> bool {
        match self.dl_set_thread_dpi_awareness_context {
            Some(set_thread_dpi_awareness_context) => {
                unsafe {
                    set_thread_dpi_awareness_context(-3i32 as _);
                }
                true
            }
            None => false,
        }
    }

    pub fn try_get_dpi_for_window(&self, window: HWND) -> u32 {
        match self.dl_get_dpi_for_window {
            Some(get_dpi_for_window) => unsafe { get_dpi_for_window(window) },
            None => USER_DEFAULT_SCREEN_DPI,
        }
    }

    pub fn register_pacer(&self, window: HWND) {
        let _ = self.loop_sender.send((window as usize, true));
    }

    pub fn unregister_pacer(&self, window: HWND) {
        let _ = self.loop_sender.send((window as usize, false));
    }

    fn create() -> Result<Arc<Self>, Error> {
        unsafe {
            let (loop_sender, loop_receiver) = channel();

            let conn = Arc::new(Self {
                cursor_cache: CursorCache::load(),
                loop_sender,

                keyboard_hook: SetWindowsHookExW(
                    WH_GETMESSAGE,
                    Some(keyboard_hook_proc),
                    hinstance(),
                    GetCurrentThreadId(),
                ),

                dl_set_thread_dpi_awareness_context: load_function_dynamic(
                    "user32.dll",
                    "SetThreadDpiAwarenessContext",
                ),
                dl_get_dpi_for_window: load_function_dynamic("user32.dll", "GetDpiForWindow"),
            });

            run_pacer_loop(loop_receiver);
            Ok(conn)
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.keyboard_hook);
        }
    }
}

fn run_pacer_loop(loop_receiver: Receiver<(usize, bool)>) {
    let mut pacers = vec![];

    spawn(move || {
        loop {
            match loop_receiver.try_recv() {
                Ok((hwnd, true)) => {
                    pacers.push(hwnd);
                }
                Ok((hwnd, false)) => {
                    if let Some(index) = pacers.iter().position(|x| *x == hwnd) {
                        pacers.swap_remove(index);
                    }
                }
                Err(TryRecvError::Disconnected) => break,
                Err(TryRecvError::Empty) => {}
            }

            for hwnd in pacers.iter() {
                let hwnd = *hwnd as HWND;
                unsafe {
                    SendMessageW(hwnd, WM_USER_FRAME_PACER, 0, 0);
                }
            }

            wait_vsync();
        }
    });
}

unsafe extern "system" fn keyboard_hook_proc(
    n_code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if n_code == HC_ACTION as i32 && wparam == PM_REMOVE as usize {
            let message = lparam as *mut MSG;

            if matches!((*message).message, WM_KEYDOWN | WM_KEYUP)
                && WindowImpl::is_our_window((*message).hwnd)
            {
                let capture = SendMessageW(
                    (*message).hwnd,
                    if (*message).message == WM_KEYDOWN {
                        WM_USER_KEY_DOWN
                    } else {
                        WM_USER_KEY_UP
                    },
                    (*message).wParam,
                    (*message).lParam,
                ) != 0;

                if capture {
                    *message = MSG {
                        message: WM_USER,
                        ..zeroed()
                    };

                    return 0;
                }
            }
        }

        CallNextHookEx(null_mut(), n_code, wparam, lparam)
    }
}

struct CursorCache {
    arrow: HCURSOR,
    cross: HCURSOR,
    hand: HCURSOR,
    help: HCURSOR,
    ibeam: HCURSOR,
    no: HCURSOR,
    size_all: HCURSOR,
    size_ns: HCURSOR,
    size_ew: HCURSOR,
    size_nesw: HCURSOR,
    size_nwse: HCURSOR,
    wait: HCURSOR,
}

impl CursorCache {
    fn load() -> Self {
        fn load_cursor(name: PCWSTR) -> HCURSOR {
            unsafe { LoadCursorW(null_mut(), name) }
        }

        Self {
            arrow: load_cursor(IDC_ARROW),
            cross: load_cursor(IDC_CROSS),
            hand: load_cursor(IDC_HAND),
            help: load_cursor(IDC_HELP),
            ibeam: load_cursor(IDC_IBEAM),
            size_all: load_cursor(IDC_SIZEALL),
            no: load_cursor(IDC_NO),
            size_ns: load_cursor(IDC_SIZENS),
            size_ew: load_cursor(IDC_SIZEWE),
            size_nesw: load_cursor(IDC_SIZENESW),
            size_nwse: load_cursor(IDC_SIZENWSE),
            wait: load_cursor(IDC_WAIT),
        }
    }

    fn get(&self, cursor: MouseCursor) -> HCURSOR {
        match cursor {
            MouseCursor::Default => self.arrow,
            MouseCursor::Help => self.help,
            MouseCursor::Cell => self.cross,
            MouseCursor::Crosshair => self.cross,
            MouseCursor::Text => self.ibeam,
            MouseCursor::VerticalText => self.ibeam, // TODO
            MouseCursor::Alias => self.arrow,        // TODO
            MouseCursor::Copy => self.arrow,         // TODO
            MouseCursor::Move => self.size_all,
            MouseCursor::PtrNotAllowed => self.no,
            MouseCursor::NotAllowed => self.no,
            MouseCursor::EResize => self.size_ew,
            MouseCursor::NResize => self.size_ns,
            MouseCursor::NeResize => self.size_nesw,
            MouseCursor::NwResize => self.size_nwse,
            MouseCursor::SResize => self.size_ns,
            MouseCursor::SeResize => self.size_nwse,
            MouseCursor::SwResize => self.size_nesw,
            MouseCursor::WResize => self.size_ew,
            MouseCursor::EwResize => self.size_ew,
            MouseCursor::NsResize => self.size_ns,
            MouseCursor::NeswResize => self.size_nesw,
            MouseCursor::NwseResize => self.size_nwse,
            MouseCursor::ColResize => self.size_ew, // TODO
            MouseCursor::RowResize => self.size_ns, // TODO
            MouseCursor::AllScroll => self.size_all,
            MouseCursor::ZoomIn => self.size_all,  // TODO
            MouseCursor::ZoomOut => self.size_all, // TODO
            MouseCursor::Hand => self.hand,
            MouseCursor::HandGrabbing => self.size_all,
            MouseCursor::Working => self.wait,
            MouseCursor::PtrWorking => self.wait,
            MouseCursor::Hidden => null_mut(),
        }
    }
}
