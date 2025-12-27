#![allow(deprecated)]

use crate::GainPluginShared;
use clack_plugin::plugin::PluginError;
use picoview::{
    Event, GlConfig, WindowBuilder, WindowWaker,
    rwh_06::{HasRawWindowHandle, WindowHandle},
};

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
            let start = std::time::Instant::now();
            Box::new(move |event| match event {
                Event::WindowFrame { gl: Some(gl) } => unsafe {
                    let time = start.elapsed().as_secs_f32();

                    let clear_color: unsafe extern "system" fn(f32, f32, f32, f32) =
                        std::mem::transmute(gl.get_proc_address(c"glClearColor"));
                    let clear: unsafe extern "system" fn(i32) =
                        std::mem::transmute(gl.get_proc_address(c"glClear"));

                    gl.make_current(true);

                    (clear_color)(
                        (time + 0.0).sin().abs(),
                        (time + 2.0).sin().abs(),
                        (time + 4.0).sin().abs(),
                        1.0,
                    );
                    (clear)(0x00004000);

                    gl.swap_buffers();
                    gl.make_current(false);
                },

                Event::Wakeup | Event::WindowClose => {
                    window.close();
                }

                _ => {}
            })
        })
        .with_opengl(GlConfig::default())
        .with_size((400, 200))
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
