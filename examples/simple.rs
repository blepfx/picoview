use picoview::{Event, EventResponse, MouseCursor, Point, Size, WindowBuilder};
use std::{
    thread,
    time::{Duration, Instant},
};

fn main() {
    WindowBuilder::new({
        let start = Instant::now();
        let mut last = Instant::now();

        move |event, mut window| {
            if matches!(event, Event::WindowOpen) {
                window.set_cursor_icon(MouseCursor::Move);
                println!("clipboard contents: {:?}", window.get_clipboard_text());
                window.set_clipboard_text("delta");
            } else if matches!(event, Event::WindowFrame { .. }) {
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

                    unsafe {
                        WindowBuilder::new(|event, _| {
                            if !matches!(event, Event::WindowFrame { .. }) {
                                println!("child {:?}", event);
                            }

                            EventResponse::Captured
                        })
                        .open_parented(window.handle())
                        .unwrap();
                    }
                }

                if passed(15000) {
                    println!("Closing window");
                    window.close();
                }

                last = Instant::now();
            } else if let Event::MouseMove {
                cursor: Some(cursor),
            } = event
            {
                if cursor.x < -10.0 {
                    window.set_cursor_position(Point { x: 100.0, y: 100.0 });
                }

                println!("{:?}", event);
            } else {
                println!("{:?}", event);
            }

            picoview::EventResponse::Rejected
        }
    })
    .with_title("picoview - simple")
    .with_size((200, 200))
    .open_blocking()
    .unwrap();

    println!("Exiting loop");

    thread::sleep(Duration::from_millis(5000));
    println!("Closing app");
}
