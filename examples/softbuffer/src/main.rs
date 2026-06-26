use picoview::{Window, WindowBuilder, WindowHandler};
use softbuffer::{Context, Surface};
use std::num::NonZero;

fn main() {
    WindowBuilder::new(|window| {
        window.set_title("Softbuffer Example");
        window.set_size((600, 600));
        window.set_max_size((1000, 1000));
        window.set_visible(true);

        let context = Context::new(window).unwrap();
        let mut surface = Surface::new(&context, window).unwrap();

        surface
            .resize(NonZero::new(600).unwrap(), NonZero::new(600).unwrap())
            .unwrap();

        Ok(Box::new(Handler {
            window,
            surface,
            damage: false,
        }))
    })
    .with_transparency(true)
    .open_blocking()
    .unwrap();
}

struct Handler<'a> {
    window: Window<'a>,
    surface: Surface<Window<'a>, Window<'a>>,
    damage: bool,
}

impl WindowHandler for Handler<'_> {
    fn close(&mut self) {
        self.window.close();
    }

    fn frame(&mut self) {
        if !self.damage {
            return;
        }

        let mut buffer = self.surface.buffer_mut().unwrap();
        for y in 0..buffer.height().get() {
            for x in 0..buffer.width().get() {
                let alpha = x % 256;
                let red = x % 256;
                let green = y % 256;
                let blue = (x * y) % 256;
                let index = y * buffer.width().get() + x;

                buffer[index as usize] = (blue * alpha / 256)
                    | ((green * alpha / 256) << 8)
                    | ((red * alpha / 256) << 16)
                    | (alpha << 24);
            }
        }

        buffer.present().unwrap();
        self.damage = false;
    }

    fn damage(&mut self, _region: picoview::Rect) {
        self.damage = true;
    }

    fn size_changed(&mut self, size: picoview::Size) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.surface
            .resize(
                NonZero::new(size.width).unwrap(),
                NonZero::new(size.height).unwrap(),
            )
            .unwrap();

        self.damage = true;
    }
}
