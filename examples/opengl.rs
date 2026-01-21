use picoview::{Event, GlConfig, Point, WindowBuilder};
use std::{mem::transmute, time::Instant};

fn main() {
    WindowBuilder::new(|window| {
        let gl = window.opengl().expect("failed to get OpenGL context");
        let clear_color: unsafe extern "system" fn(f32, f32, f32, f32) =
            unsafe { transmute(gl.get_proc_address(c"glClearColor")) };
        let clear: unsafe extern "system" fn(i32) =
            unsafe { transmute(gl.get_proc_address(c"glClear")) };

        let mut last_frame = Instant::now();
        let mut time = 0.0;

        Box::new(move |event| match event {
            Event::WindowFrame => unsafe {
                time += last_frame.elapsed().as_secs_f32();
                last_frame = Instant::now();

                gl.make_current(true);

                (clear_color)(
                    (time + 0.0).sin().abs(),
                    (time + 2.0).sin().abs(),
                    (time + 4.0).sin().abs(),
                    (time + 6.0).sin().abs(),
                );
                (clear)(0x00004000);

                gl.swap_buffers();
                gl.make_current(false);
            },

            Event::MouseMove { relative, .. } => {
                if relative.x < -10.0 {
                    window.set_cursor_position(Point { x: 100.0, y: 100.0 });
                }
            }

            Event::WindowClose => {
                println!("{:?}", event);
                window.close();
            }

            event => println!("{:?}", event),
        })
    })
    .with_opengl(GlConfig::default())
    .with_size((200, 200))
    .with_resizable((0, 0), (1000, 1000))
    .with_transparency(true)
    .open_blocking()
    .expect("failed to open a window");
}
