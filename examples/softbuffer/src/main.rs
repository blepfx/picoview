use std::num::NonZero;

use picoview::{Event, WindowBuilder};
use softbuffer::{Context, Rect, Surface};

fn main() {
    WindowBuilder::new(|window| {
        let context = Context::new(window).unwrap();
        let mut surface = Surface::new(&context, window).unwrap();
        surface
            .resize(NonZero::new(600).unwrap(), NonZero::new(600).unwrap())
            .unwrap();

        Box::new(move |event| match event {
            Event::WindowDamage { x, y, w, h } => {
                let mut buffer = surface.buffer_mut().unwrap();
                for y in y..(y + h) {
                    for x in x..(x + w) {
                        let red = x % 255;
                        let green = y % 255;
                        let blue = (x * y) % 255;
                        let index = y * buffer.width().get() + x;
                        buffer[index as usize] = blue | (green << 8) | (red << 16);
                    }
                }

                buffer
                    .present_with_damage(&[Rect {
                        x,
                        y,
                        width: NonZero::new(w).unwrap(),
                        height: NonZero::new(h).unwrap(),
                    }])
                    .unwrap();
            }

            Event::WindowResize { size } => {
                surface
                    .resize(
                        NonZero::new(size.width).unwrap(),
                        NonZero::new(size.height).unwrap(),
                    )
                    .unwrap();
            }

            Event::WindowClose => {
                window.close();
            }

            _ => {}
        })
    })
    .with_title("Softbuffer Example")
    .with_size((600, 600))
    .with_resizable((0, 0), (1000, 1000))
    .open_blocking()
    .unwrap();
}
