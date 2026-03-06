use picoview::{Event, GlConfig, GlVersion, Point, Size, WindowBuilder};
use std::mem::transmute;

fn main() {
    WindowBuilder::new(|window| {
        let gl = window.opengl().expect("failed to get OpenGL context");
        let clear_color: unsafe extern "system" fn(f32, f32, f32, f32) =
            unsafe { transmute(gl.get_proc_address(c"glClearColor")) };
        let clear: unsafe extern "system" fn(i32) =
            unsafe { transmute(gl.get_proc_address(c"glClear")) };
        let begin: unsafe extern "system" fn(i32) =
            unsafe { transmute(gl.get_proc_address(c"glBegin")) };
        let end: unsafe extern "system" fn() = unsafe { transmute(gl.get_proc_address(c"glEnd")) };
        let vertex: unsafe extern "system" fn(f32, f32, f32) =
            unsafe { transmute(gl.get_proc_address(c"glVertex3f")) };
        let viewport: unsafe extern "system" fn(i32, i32, i32, i32) =
            unsafe { transmute(gl.get_proc_address(c"glViewport")) };
        let color: unsafe extern "system" fn(f32, f32, f32, f32) =
            unsafe { transmute(gl.get_proc_address(c"glColor4f")) };

        let mut size = Size {
            width: 200,
            height: 200,
        };

        Box::new(move |event| match event {
            Event::WindowFrame => unsafe {
                gl.make_current(true).unwrap();

                (clear_color)(0.25, 0.25, 0.25, 0.5);
                (clear)(0x00004000);
                (viewport)(0, 0, size.width as i32, size.height as i32);

                let draw_line = |x1: f32, y1: f32, x2: f32, y2: f32| {
                    let x1 = (x1 / size.width as f32) * 2.0 - 1.0;
                    let y1 = 1.0 - (y1 / size.height as f32) * 2.0;
                    let x2 = (x2 / size.width as f32) * 2.0 - 1.0;
                    let y2 = 1.0 - (y2 / size.height as f32) * 2.0;

                    (vertex)(x1, y1, 0.0);
                    (vertex)(x2, y2, 0.0);
                };

                (begin)(0x0001); // GL_LINES

                (color)(0.5, 0.5, 0.5, 0.5);
                for i in (0..1000).step_by(25) {
                    draw_line(i as f32, 0.0, i as f32, size.height as f32);
                    draw_line(0.0, i as f32, size.width as f32, i as f32);
                }

                (color)(1.0, 1.0, 1.0, 1.0);
                for i in (0..1000).step_by(100) {
                    draw_line(i as f32, 0.0, i as f32, size.height as f32);
                    draw_line(0.0, i as f32, size.width as f32, i as f32);
                }

                (end)();

                gl.swap_buffers().unwrap();
                gl.make_current(false).unwrap();
            },

            Event::MouseMove { relative, .. } => {
                println!("{:?}", event);

                if relative.x < -10.0 {
                    window.set_cursor_position(Point { x: 100.0, y: 100.0 });
                }
            }

            Event::WindowClose => {
                println!("{:?}", event);
                window.close();
            }

            Event::WindowResize { size: new_size } => {
                size = new_size;
            }

            event => println!("{:?}", event),
        })
    })
    .with_title("OpenGL Example")
    .with_opengl(GlConfig {
        version: GlVersion::Compat(2, 1),
        ..Default::default()
    })
    .with_size((200, 200))
    .with_resizable((0, 0), (1000, 1000))
    .with_transparency(true)
    .open_blocking()
    .expect("failed to open a window");
}
