#![allow(deprecated)]

use std::time::Instant;

use crate::GainPluginShared;
use clack_plugin::plugin::PluginError;
use picoview::rwh_06::{HasRawWindowHandle, WindowHandle};
use picoview::{GlConfig, Window, WindowBuilder, WindowHandler, WindowWaker};

#[derive(Default)]
pub struct GainPluginGui {
    window: Option<WindowWaker>,
}

impl GainPluginGui {
    pub fn open(
        &mut self,
        _state: &GainPluginShared,
        parent: clack_extensions::gui::Window<'_>,
    ) -> Result<(), PluginError> {
        WindowBuilder::new(|window| {
            window.set_title("Gain Plugin");
            window.set_size((400, 200));
            window.set_visible(true);

            Ok(Box::new(Handler {
                window,
                start: Instant::now(),
            }))
        })
        .with_opengl(GlConfig::default())
        .open_embedded(unsafe { WindowHandle::borrow_raw(parent.raw_window_handle().unwrap()) })
        .unwrap();

        Ok(())
    }

    pub fn close(&mut self) {
        if let Some(window) = self.window.take() {
            window.wakeup().unwrap();
        }
    }
}

struct Handler<'a> {
    window: Window<'a>,
    start: Instant,
}

impl WindowHandler for Handler<'_> {
    fn frame(&mut self) {
        let time = self.start.elapsed().as_secs_f32();

        let gl = self.window.opengl().expect("failed to get OpenGL context");
        let clear_color: unsafe extern "system" fn(f32, f32, f32, f32) =
            unsafe { std::mem::transmute(gl.get_proc_address(c"glClearColor")) };
        let clear: unsafe extern "system" fn(i32) =
            unsafe { std::mem::transmute(gl.get_proc_address(c"glClear")) };

        gl.make_current(true).unwrap();

        unsafe {
            (clear_color)(
                (time + 0.0).sin().abs(),
                (time + 2.0).sin().abs(),
                (time + 4.0).sin().abs(),
                1.0,
            );
            (clear)(0x00004000);
        }

        gl.swap_buffers().unwrap();
        gl.make_current(false).unwrap();
    }

    fn close(&mut self) {
        self.window.close();
    }
}
