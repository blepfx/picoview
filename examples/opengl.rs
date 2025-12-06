use picoview::{Event, GlConfig, GlFormat, GlVersion, Point, WindowBuilder};
use std::{
    mem::transmute,
    time::{Duration, Instant},
};

fn main() {
    WindowBuilder::new(|window| {
        let mut last_frame = Instant::now();

        Box::new(move |event| match event {
            Event::WindowFrame { gl: Some(gl) } => unsafe {
                println!("{:?}", last_frame.elapsed());
                last_frame = Instant::now();

                let clear_color: unsafe extern "system" fn(f32, f32, f32, f32) =
                    transmute(gl.get_proc_address(c"glClearColor"));
                let clear: unsafe extern "system" fn(i32) =
                    transmute(gl.get_proc_address(c"glClear"));

                gl.make_current(true);

                (clear_color)(1.0, 1.0, 0.0, 0.01);
                (clear)(0x00004000);

                gl.swap_buffers();
                gl.make_current(false);

                window.sleep(Instant::now() + Duration::from_millis(50));
            },

            Event::MouseMove { relative, .. } => {
                if relative.x < -10.0 {
                    window.set_cursor_position(Point { x: 100.0, y: 100.0 });
                }
            }

            Event::WindowResize { size } => {
                window.sleep(Instant::now());
                println!("{:?}", size);
            }

            Event::WindowMove { origin } => {
                println!("{:?}", origin);
            }

            _ => {}
        })
    })
    .with_opengl(GlConfig {
        version: GlVersion::Core(3, 1),
        format: GlFormat::RGBA8_D24,
        optional: false,
        msaa_count: 0,
        debug: cfg!(debug_assertions),
        ..Default::default()
    })
    .with_size((200, 200))
    .with_resizable((0, 0), (1000, 1000))
    .open_blocking()
    .expect("failed to open a window");
}
