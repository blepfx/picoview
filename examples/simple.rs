use picoview::{Event, MouseCursor, Point, Size, Window, WindowBuilder};
use std::time::{Duration, Instant};

fn main() {
    WindowBuilder::new(|mut window| {
        let start = Instant::now();
        let mut last = Instant::now();

        window.set_cursor_icon(MouseCursor::Move);
        println!("clipboard contents: {:?}", window.get_clipboard_text());
        window.set_clipboard_text("test");

        // you *have* to make both types explicit otherwise rust complains
        move |event: Event<'_>, mut window: Window<'_>| match event {
            Event::WindowFrame { .. } => {
                let passed = |d| {
                    start.elapsed() >= Duration::from_millis(d)
                        && (last - start) < Duration::from_millis(d)
                };

                if passed(5000) {
                    println!("Resize window");
                    window.set_title("picoview - example");
                    window.set_size(Size {
                        width: 300,
                        height: 300,
                    });

                    WindowBuilder::new(|mut window| {
                        window.set_keyboard_input(true);

                        |event: Event<'_>, _: Window<'_>| {
                            if !matches!(event, Event::WindowFrame { .. }) {
                                println!("child {:?}", event);
                            }
                        }
                    })
                    .open_parented(&window)
                    .unwrap();
                }

                if passed(15000) {
                    println!("Closing window");
                    window.close();
                }

                last = Instant::now();
            }

            Event::MouseMove { cursor } => {
                if let Some(cursor) = cursor {
                    if cursor.x < -10.0 {
                        window.set_cursor_icon(MouseCursor::Hidden);
                        window.set_cursor_position(Point { x: 100.0, y: 100.0 });
                    }
                }

                println!("{:?}", event);
            }

            _ => {
                println!("{:?}", event);
            }
        }
    })
    .with_title("picoview - simple")
    .with_size((200, 200))
    .with_position((1000, 100))
    .open_blocking()
    .unwrap();

    println!("Exiting loop");
}
