use super::{event_loop::WM_USER_FRAME_TIMER, util::is_windows10_or_greater};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::sleep,
    time::Duration,
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    Graphics::{
        Dwm::{DwmFlush, DwmIsCompositionEnabled},
        Dxgi::{CreateDXGIFactory, IDXGIFactory, IDXGIOutput},
        Gdi::{MonitorFromWindow, MONITOR_DEFAULTTOPRIMARY},
    },
    UI::WindowsAndMessaging::SendMessageW,
};

struct Inner {
    running: AtomicBool,
    moved: AtomicBool,
}

pub struct PacerThread(Arc<Inner>);

impl PacerThread {
    pub fn new(hwnd: HWND) -> Self {
        let data = Arc::new(Inner {
            running: AtomicBool::new(true),
            moved: AtomicBool::new(true),
        });

        std::thread::spawn({
            let data = data.clone();
            let hwnd = hwnd.0 as usize;
            move || unsafe {
                let hwnd = HWND(hwnd as _);
                let mut dxgi = None;
                while data.running.load(Ordering::Relaxed) {
                    if data.moved.swap(false, Ordering::Relaxed) {
                        dxgi = create_dxgi(hwnd);
                    }

                    let waited = match &dxgi {
                        Some(dxgi) => dxgi.WaitForVBlank().is_ok(),
                        None => {
                            DwmIsCompositionEnabled().unwrap_or_default().as_bool()
                                && DwmFlush().is_ok()
                        }
                    };

                    if !waited {
                        sleep(Duration::from_millis(16));
                    }

                    SendMessageW(hwnd, WM_USER_FRAME_TIMER, WPARAM(0), LPARAM(0));
                }
            }
        });

        Self(data)
    }

    pub fn mark_moved(&self) {
        self.0.running.store(true, Ordering::Relaxed);
    }

    pub fn mark_dead(&self) {
        self.0.running.store(false, Ordering::Relaxed);
    }
}

unsafe fn create_dxgi(hwnd: HWND) -> Option<IDXGIOutput> {
    if is_windows10_or_greater() {
        let dxgi_factory = CreateDXGIFactory::<IDXGIFactory>().unwrap();
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut adapter_index = 0;
        while let Ok(adapter) = dxgi_factory.EnumAdapters(adapter_index) {
            let mut output_index = 0;
            while let Ok(output) = adapter.EnumOutputs(output_index) {
                let desc = output.GetDesc().unwrap();
                if desc.Monitor == monitor {
                    return Some(output);
                }

                output_index += 1;
            }

            adapter_index += 1;
        }
    }

    None
}
