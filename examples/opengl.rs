use picoview::{Event, GlConfig, Point, Window, WindowBuilder, WindowHandler};
use std::mem::transmute;

pub struct MyApp {
    window: Window,
}

impl WindowHandler for MyApp {
    fn window<'a>(&'a self) -> &'a Window {
        &self.window
    }

    fn on_event(&mut self, event: Event) -> picoview::EventResponse {
        match event {
            Event::WindowFrame { gl: Some(gl) } => unsafe {
                let clear_color: unsafe extern "system" fn(f32, f32, f32, f32) =
                    transmute(gl.get_proc_address(c"glClearColor"));
                let clear: unsafe extern "system" fn(i32) =
                    transmute(gl.get_proc_address(c"glClear"));

                (clear_color)(1.0, 1.0, 0.0, 0.5);
                (clear)(0x00004000);

                gl.swap_buffers();
            },

            Event::MouseMove {
                cursor: Some(cursor),
            } => {
                if cursor.x < -10.0 {
                    self.window
                        .set_cursor_position(Point { x: 100.0, y: 100.0 });
                }
            }

            _ => {}
        }

        picoview::EventResponse::Rejected
    }
}

fn main() {
    WindowBuilder::new(|window| MyApp { window })
        .with_opengl(GlConfig::default())
        .with_size((200, 200))
        .open_blocking()
        .unwrap();
}
