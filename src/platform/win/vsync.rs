use crate::platform::win::window::WM_USER_VSYNC;
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
    UI::WindowsAndMessaging::SendMessageW,
};

pub struct VSyncCallback(Arc<Inner>);

impl VSyncCallback {
    pub unsafe fn new(hwnd: HWND) -> Self {
        let inner = Arc::new(Inner {
            active: AtomicBool::new(true),
            notify_moved: AtomicBool::new(false),
            notify_display_change: AtomicBool::new(true),
        });

        std::thread::spawn({
            let hwnd = hwnd as usize;
            let inner = inner.clone();
            move || unsafe { run_vsync_thread(hwnd as HWND, inner) }
        });

        Self(inner)
    }

    pub fn notify_moved(&self) {
        self.0.notify_moved.store(true, Ordering::Relaxed);
    }

    pub fn notify_display_change(&self) {
        self.0.notify_display_change.store(true, Ordering::Relaxed);
    }
}

impl Drop for VSyncCallback {
    fn drop(&mut self) {
        self.0.active.store(false, Ordering::Relaxed);
    }
}

struct Inner {
    active: AtomicBool,
    notify_moved: AtomicBool,
    notify_display_change: AtomicBool,
}

// TODO: add dxgi waitforvblank, fallback to dwm
unsafe fn run_vsync_thread(hwnd: HWND, sync: Arc<Inner>) {
    unsafe {
        let mut current_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut fallback_next_frame = Instant::now();
        let mut fallback_interval = Duration::from_millis(15);

        while sync.active.load(Ordering::Relaxed) {
            let mut monitor_changed = sync.notify_display_change.swap(false, Ordering::Relaxed);
            if sync.notify_moved.swap(false, Ordering::Relaxed) {
                let new_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
                monitor_changed |= current_monitor != new_monitor;
            }

            if monitor_changed {
                current_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
                fallback_interval = Duration::from_secs_f32(
                    1.0 / get_refresh_rate(current_monitor).unwrap_or(60) as f32,
                );
            };

            if !wait_dwm_flush() {
                wait_fallback(&mut fallback_next_frame, fallback_interval);
            }

            SendMessageW(hwnd, WM_USER_VSYNC, 0, 0);
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

        Some(devmode.dmDisplayFrequency)
    }
}
