use picoview::{Event, WindowBuilder};
use softbuffer::{Context, Surface};
use std::num::NonZero;

fn main() {
    WindowBuilder::new(|window| {
        let context = Context::new(window).unwrap();
        let mut surface = Surface::new(&context, window).unwrap();
        let mut damage = false;

        surface
            .resize(NonZero::new(600).unwrap(), NonZero::new(600).unwrap())
            .unwrap();

        Box::new(move |event| match event {
            Event::WindowDamage { x, y, w, h } => {
                println!("Damage: x={} y={} w={} h={}", x, y, w, h);
                damage = true;
            }

            Event::WindowFrame { .. } => {
                if !damage {
                    return;
                }

                let mut buffer = surface.buffer_mut().unwrap();
                for y in 0..buffer.height().get() {
                    for x in 0..buffer.width().get() {
                        let red = x % 256;
                        let green = y % 256;
                        let blue = (x * y) % 256;
                        let index = y * buffer.width().get() + x;
                        buffer[index as usize] = blue | (green << 8) | (red << 16);
                    }
                }

                buffer.present().unwrap();
                damage = false;
            }

            Event::WindowResize { size } => {
                println!("Resize: width={} height={}", size.width, size.height);

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
