use picoview::{
    Event, EventResponse, MouseCursor, Point, Size, Window, WindowBuilder, WindowHandler,
};
use std::time::{Duration, Instant};

pub struct Child {
    window: Window,
}

impl WindowHandler for Child {
    fn window<'a>(&'a self) -> &'a Window {
        &self.window
    }

    fn on_event(&mut self, event: Event) -> EventResponse {
        if !matches!(event, Event::WindowFrame { .. }) {
            println!("child {:?}", event);
        }

        EventResponse::Captured
    }
}

pub struct MyApp {
    start: Instant,
    last: Instant,
    window: Window,
}

impl WindowHandler for MyApp {
    fn window<'a>(&'a self) -> &'a Window {
        &self.window
    }

    fn on_event(&mut self, event: Event) -> EventResponse {
        match event {
            Event::WindowOpen => {
                self.window.set_cursor_icon(MouseCursor::Move);
                println!("clipboard contents: {:?}", self.window.get_clipboard_text());
                self.window.set_clipboard_text("test");
            }

            Event::WindowFrame { .. } => {
                let passed = |d| {
                    self.start.elapsed() >= Duration::from_millis(d)
                        && (self.last - self.start) < Duration::from_millis(d)
                };

                if passed(5000) {
                    println!("Resize window");
                    self.window.set_title("don't talk to me or my");
                    self.window.set_size(Size {
                        width: 300,
                        height: 300,
                    });

                    WindowBuilder::new(|window| Child { window })
                        .with_title("son")
                        .with_position((150, 150))
                        .open_parented(self.window.handle())
                        .unwrap();
                }

                if passed(15000) {
                    println!("Closing window");
                    self.window.close();
                }

                self.last = Instant::now();
            }

            Event::MouseMove { cursor } => {
                if let Some(cursor) = cursor
                    && cursor.x < -10.0
                {
                    self.window
                        .set_cursor_position(Point { x: 100.0, y: 100.0 });
                }

                println!("{:?}", event);
            }

            _ => {
                println!("{:?}", event);
            }
        }

        picoview::EventResponse::Rejected
    }
}

fn main() {
    WindowBuilder::new(|window| MyApp {
        start: Instant::now(),
        last: Instant::now(),
        window,
    })
    .with_title("picoview - simple")
    .with_size((200, 200))
    .with_position((150, 150))
    .open_blocking()
    .unwrap();

    println!("Exiting loop");
}
