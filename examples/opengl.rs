use picoview::{Event, GlConfig, GlFormat, GlVersion, Point, Window, WindowBuilder};
use std::mem::transmute;

fn main() {
    WindowBuilder::new(|_| {
        move |event: Event<'_>, mut window: Window<'_>| match event {
            Event::WindowFrame { gl: Some(gl) } => unsafe {
                let clear_color: unsafe extern "system" fn(f32, f32, f32, f32) =
                    transmute(gl.get_proc_address(c"glClearColor"));
                let clear: unsafe extern "system" fn(i32) =
                    transmute(gl.get_proc_address(c"glClear"));

                gl.make_current(true);

                (clear_color)(1.0, 1.0, 0.0, 0.5);
                (clear)(0x00004000);

                gl.swap_buffers();
                gl.make_current(false);
            },

            Event::MouseMove { relative, .. } => {
                if relative.x < -10.0 {
                    window.set_cursor_position(Point { x: 100.0, y: 100.0 });
                }
            }

            _ => {}
        }
    })
    .with_opengl(GlConfig {
        version: GlVersion::Core(3, 1),
        format: GlFormat::RGB8_D24,
        transparent: false,
        optional: false,
        msaa_count: 0,
        debug: cfg!(debug_assertions),
        ..Default::default()
    })
    .with_size((200, 200))
    .with_transparency(false)
    .open_blocking()
    .unwrap();
}
