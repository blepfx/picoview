use picoview::{Event, GlConfig, Options, Point, Size, Style, open_window};
use std::mem::transmute;

fn main() {
    open_window(Options {
        opengl: Some(GlConfig {
            transparent: true,
            ..GlConfig::default()
        }),
        parent: None,
        style: Style::VISIBLE | Style::BORDER | Style::TRANSPARENT,
        size: Size {
            width: 200,
            height: 200,
        },
        position: None,
        handler: Box::new({
            move |event, window| {
                match event {
                    Event::WindowFrame { gl: Some(gl) } => unsafe {
                        let clear_color: unsafe extern "system" fn(f32, f32, f32, f32) =
                            transmute(gl.get_proc_address(c"glClearColor"));
                        let clear: unsafe extern "system" fn(i32) = transmute(gl.get_proc_address(c"glClear"));

                        (clear_color)(1.0, 1.0, 0.0, 0.5);
                        (clear)(0x00004000);

                        gl.swap_buffers();
                    },

                    Event::MouseMove { cursor: Some(cursor) } => {
                        if cursor.x < -10.0 {
                            window.set_cursor_position(Point { x: 100.0, y: 100.0 });
                        }
                    }

                    Event::WindowClose => {
                        window.close();
                    }

                    _ => {}
                }

                picoview::EventResponse::Rejected
            }
        }),
    })
    .unwrap();
}
