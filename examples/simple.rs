use picoview::{Event, Exchange, Key, MouseCursor, Point, Size, WindowBuilder};
use std::{
    mem::replace,
    time::{Duration, Instant},
};


fn main() {
    WindowBuilder::new(|window| {
        let start = Instant::now();
        let mut last = Instant::now();

        window.set_cursor_icon(MouseCursor::Move);
        println!("clipboard contents: {:?}", window.get_clipboard());
        window.set_clipboard(Exchange::Text("Hello from picoview!".to_string()));
        println!("clipboard contents: {:?}", window.get_clipboard());

        Box::new(move |event| match event {
            Event::WindowFrame => {
                let current = Instant::now();
                let last = replace(&mut last, current);

                let passed = |d| {
                    (current - start) >= Duration::from_millis(d)
                        && (last - start) < Duration::from_millis(d)
                };

                if passed(10000) {
                    println!("Resize window");
                    window.set_title("picoview - example");
                    window.set_size(Size {
                        width: 300,
                        height: 300,
                    });

                    println!("Child window requested");
                    let waker = WindowBuilder::new(|window| {
                        println!("Child window opened");

                        Box::new(move |event| {
                            if let Event::KeyDown { key, capture } = event {
                                if key == Key::Enter {
                                    *capture = false;
                                } else if key == Key::Escape {
                                    *capture = true;
                                    window.close();
                                }
                            } else if !matches!(event, Event::WindowFrame) {
                                println!("child {:?}", event);
                            }
                        })
                    })
                    .open_embedded(window)
                    .expect("failed to open a child window");

                    waker.wakeup().unwrap();
                }

                if passed(30000) {
                    println!("Closing window");
                    window.close();
                }
            }

            // you have to handle WindowClose explicitly to close the window
            Event::WindowClose => {
                println!("{:?}", event);
                window.close();
            }

            Event::MouseMove { relative, .. } => {
                if relative.x < -10.0 {
                    window.set_cursor_icon(MouseCursor::Hidden);
                    window.set_cursor_position(Point { x: 100.0, y: 100.0 });
                } else {
                    window.set_cursor_icon(MouseCursor::Default);
                }

                println!("{:?}", event);
            }

            _ => {
                println!("{:?}", event);
            }
        })
    })
    .with_title("picoview - simple")
    .with_size((200, 200))
    .with_position((1000, 100))
    .open_blocking()
    .expect("failed to open a window");

    println!("Exiting loop");
}
