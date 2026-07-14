use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE, HWND, RECT};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, WINDOW_EX_STYLE, WINDOW_STYLE,
};
use windows_sys::core::BOOL;

/// A context for managing DPI awareness and querying DPI information on
/// Windows.
#[derive(Default)]
pub struct DpiContext {
    user32: HMODULE,
    get_dpi_for_window: Option<unsafe extern "system" fn(HWND) -> u32>,
    set_thread_dpi_awareness_context: Option<unsafe extern "system" fn(isize) -> isize>,
    adjust_window_rect_ex_for_dpi: Option<
        unsafe extern "system" fn(*mut RECT, WINDOW_STYLE, BOOL, WINDOW_EX_STYLE, u32) -> BOOL,
    >,
}

/// A RAII guard that sets the DPI awareness for the current thread, and
/// restores the previous DPI awareness state when dropped.
pub struct DpiAwarenessGuard<'a> {
    context: &'a DpiContext,
    previous: isize,
}

impl DpiContext {
    /// Creates a new [`DpiContext`], loading the necessary functions from
    /// `user32.dll` at runtime (so it can work on older versions of Windows,
    /// like Windows 7, where DPI awareness functions are not available).
    pub fn new() -> Self {
        unsafe {
            let user32 = LoadLibraryA(c"user32.dll".as_ptr() as *const _);
            if user32.is_null() {
                // weird, but okay, just return a default context that does nothing
                return Self::default();
            }

            let set_thread_dpi_awareness_context =
                GetProcAddress(user32, c"SetThreadDpiAwarenessContext".as_ptr() as *const _)
                    .map(|x| std::mem::transmute_copy(&x));
            let get_dpi_for_window =
                GetProcAddress(user32, c"GetDpiForWindow".as_ptr() as *const _)
                    .map(|x| std::mem::transmute_copy(&x));
            let adjust_window_rect_ex_for_dpi =
                GetProcAddress(user32, c"AdjustWindowRectExForDpi".as_ptr() as *const _)
                    .map(|x| std::mem::transmute_copy(&x));

            Self {
                user32,
                get_dpi_for_window,
                set_thread_dpi_awareness_context,
                adjust_window_rect_ex_for_dpi,
            }
        }
    }

    /// Gets the DPI scale for the given window, if available. Returns `None` if
    /// the function is not available.
    ///
    /// # Safety
    /// - The `hwnd` must be a valid window handle for the lifetime of the call.
    pub unsafe fn dpi_for_window(&self, hwnd: HWND) -> Option<u32> {
        self.get_dpi_for_window.map(|f| unsafe { f(hwnd) })
    }

    /// Calculates the required window rectangle for a given client rectangle,
    /// taking into account the window styles and DPI scaling.
    ///
    /// Uses `AdjustWindowRectExForDpi` if available, otherwise falls back to
    /// `AdjustWindowRectEx`.
    pub fn adjust_window_rect_ex_for_dpi(
        &self,
        mut rect: RECT,
        style: WINDOW_STYLE,
        ex_style: WINDOW_EX_STYLE,
        has_menu: bool,
        dpi: u32,
    ) -> Option<RECT> {
        if let Some(adjust) = self.adjust_window_rect_ex_for_dpi {
            let success = unsafe { adjust(&mut rect, style, has_menu as BOOL, ex_style, dpi) };
            if success != 0 { Some(rect) } else { None }
        } else {
            // fallback to the old function, which will not scale the rect for dpi
            // which is fine because then we would be dpi-unaware anyway
            let success =
                unsafe { AdjustWindowRectEx(&mut rect, style, has_menu as BOOL, ex_style) };
            if success != 0 { Some(rect) } else { None }
        }
    }

    /// Set the thread DPI awareness to
    /// `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` for the duration of the
    /// guard, if supported.
    pub fn enter_per_monitor_aware_v2(&self) -> DpiAwarenessGuard<'_> {
        /// https://learn.microsoft.com/en-us/windows/win32/hidpi/dpi-awareness-context
        const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

        DpiAwarenessGuard {
            context: self,
            previous: self
                .set_thread_dpi_awareness_context
                .map(|f| unsafe { f(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) })
                .unwrap_or(0),
        }
    }
}

impl Drop for DpiAwarenessGuard<'_> {
    fn drop(&mut self) {
        if let Some(f) = self.context.set_thread_dpi_awareness_context {
            unsafe { f(self.previous) };
        }
    }
}

impl Drop for DpiContext {
    fn drop(&mut self) {
        unsafe {
            if !self.user32.is_null() {
                FreeLibrary(self.user32);
            }
        }
    }
}
