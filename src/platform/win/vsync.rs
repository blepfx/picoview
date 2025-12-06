use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::sleep,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::{
        Dwm::{DwmFlush, DwmIsCompositionEnabled},
        Gdi::{
            DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetMonitorInfoW, HMONITOR,
            MONITOR_DEFAULTTOPRIMARY, MONITORINFOEXW, MonitorFromWindow,
        },
    },
};

pub struct VSyncCallback {
    inner: Arc<Inner>,
}

impl VSyncCallback {
    pub unsafe fn new<F: FnMut(HWND) + Send + 'static>(hwnd: HWND, callback: F) -> Self {
        let inner = Arc::new(Inner {
            hwnd: hwnd as usize,
            active: AtomicBool::new(true),
            notify_display_change: AtomicBool::new(true),
        });

        std::thread::spawn({
            let inner = inner.clone();
            move || unsafe { run_vsync_thread(inner, callback) }
        });

        Self { inner }
    }

    pub fn notify_display_change(&self) {
        self.inner
            .notify_display_change
            .store(true, Ordering::Relaxed);
    }
}

impl Drop for VSyncCallback {
    fn drop(&mut self) {
        self.inner.active.store(false, Ordering::Relaxed);
    }
}

struct Inner {
    hwnd: usize,
    active: AtomicBool,
    notify_display_change: AtomicBool,
}

unsafe fn run_vsync_thread<F: FnMut(HWND)>(sync: Arc<Inner>, mut callback: F) {
    unsafe {
        let hwnd = sync.hwnd as HWND;
        let mut fallback_next_frame = Instant::now();
        let mut fallback_interval = Duration::from_millis(15);

        while sync.active.load(Ordering::Relaxed) {
            if sync.notify_display_change.swap(false, Ordering::Relaxed) {
                fallback_interval = Duration::from_secs_f32(
                    1.0 / get_refresh_rate(MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY))
                        .unwrap_or(60) as f32,
                );
            };

            if !wait_dwm_flush() {
                wait_fallback(&mut fallback_next_frame, fallback_interval);
            }

            callback(hwnd);
        }
    }
}

fn wait_dwm_flush() -> bool {
    unsafe {
        let mut pfenabled = 0;
        if DwmIsCompositionEnabled(&mut pfenabled) == 0 && pfenabled != 0 {
            DwmFlush() == 0
        } else {
            false
        }
    }
}

fn wait_fallback(next_frame: &mut Instant, interval: Duration) {
    let curr_frame = Instant::now();
    let wait_time = next_frame.checked_duration_since(curr_frame);
    *next_frame = (*next_frame + interval).max(curr_frame);

    if let Some(time) = wait_time {
        sleep(time)
    }
}

unsafe fn get_refresh_rate(monitor: HMONITOR) -> Option<u32> {
    unsafe {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as _;

        if GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _) == 0 {
            return None;
        }

        let mut devmode = DEVMODEW::default();
        if EnumDisplaySettingsW(info.szDevice.as_ptr(), ENUM_CURRENT_SETTINGS, &mut devmode) == 0 {
            return None;
        }

        if devmode.dmDisplayFrequency == 0 {
            return None;
        }

        Some(devmode.dmDisplayFrequency)
    }
}
