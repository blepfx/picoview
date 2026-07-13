use crate::platform::win::util::proc_address;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::HWND;

/// https://learn.microsoft.com/en-us/windows/win32/hidpi/dpi-awareness-context
pub const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE: isize = -3;

/// Sets the DPI awareness for the current thread, if available.
///
/// https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext
///
/// This function will load `SetThreadDpiAwarenessContext` symbol dynamically so
/// it can work on older versions of Windows (Windows 7) where the function is
/// not available. If the function is not available, it will return `None`.
///
/// # Safety
/// - The `awareness` must be a valid DPI awareness mode.
pub unsafe fn try_set_thread_dpi_awareness(awareness: isize) -> Option<isize> {
    static FUNC: OnceLock<Option<unsafe extern "system" fn(isize) -> isize>> = OnceLock::new();
    unsafe {
        FUNC.get_or_init(|| proc_address(c"user32.dll", c"SetThreadDpiAwarenessContext"))
            .map(|f| f(awareness))
    }
}

/// Returns the DPI for the given window, if available.
///
/// https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow
///
/// This function will load `GetDpiForWindow` symbol dynamically so it can work
/// on older versions of Windows (Windows 7) where the function is not
/// available. If the function is not available, it will return `None`.
///
/// # Safety
/// - The `window` must be a valid window handle for the lifetime of the call.
pub unsafe fn try_get_dpi_for_window(window: HWND) -> Option<u32> {
    static FUNC: OnceLock<Option<unsafe extern "system" fn(HWND) -> u32>> = OnceLock::new();
    unsafe {
        FUNC.get_or_init(|| proc_address(c"user32.dll", c"GetDpiForWindow"))
            .map(|f| f(window))
    }
}

/// A RAII guard that sets the DPI awareness for the current thread, and
/// restores the previous DPI awareness state when dropped.
pub struct ThreadDpiAwareness(Option<isize>);

impl ThreadDpiAwareness {
    /// Set the awareness to [`DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE`] for the
    /// current thread, if supported.
    pub fn per_monitor_aware() -> Self {
        unsafe {
            Self(try_set_thread_dpi_awareness(
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
            ))
        }
    }
}

impl Drop for ThreadDpiAwareness {
    fn drop(&mut self) {
        if let Some(prev) = self.0 {
            unsafe { try_set_thread_dpi_awareness(prev) };
        }
    }
}
