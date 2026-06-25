use picoview::{Event, Key, MouseCursor, Point, WindowBuilder};

fn main() {
    WindowBuilder::new(|window| {
        window.set_title("picoview test - embed");
        window.set_size((400, 200));
        window.set_position((1000, 100));
        window.set_visible(true);

        WindowBuilder::new(|window| {
            window.set_size((200, 200));
            window.set_visible(true);

            Box::new(move |event| {
                if let Event::KeyDown { key, capture } = event {
                    if key == Key::Enter {
                        *capture = true;
                    } else if key == Key::Escape {
                        *capture = true;
                        window.close();
                    }
                } else if !matches!(event, Event::WindowFrame) {
                    println!("l {:?}", event);
                }
            })
        })
        .open_embedded(window)
        .expect("failed to open a child window");

        WindowBuilder::new(|window| {
            window.set_size((200, 200));
            window.set_position((200, 0));
            window.set_visible(true);

            Box::new(move |event| {
                if !matches!(event, Event::WindowFrame) {
                    println!("r {:?}", event);
                }
            })
        })
        .open_embedded(window)
        .expect("failed to open a child window");

        Box::new(move |event| match event {
            Event::WindowFrame => {}

            // you have to handle WindowClose explicitly to close the window
            Event::WindowClose => {
                window.close();
            }

            Event::MouseMove { relative, .. } => {
                if relative.x < -10.0 {
                    window.set_cursor_icon(MouseCursor::Hidden);
                    window.set_cursor_position(Point { x: 100.0, y: 100.0 });
                } else {
                    window.set_cursor_icon(MouseCursor::Default);
                }

                println!("m {:?}", event);
            }

            _ => {
                println!("m {:?}", event);
            }
        })
    })
    .open_blocking()
    .expect("failed to open a window");

    println!("Exiting loop");
}
