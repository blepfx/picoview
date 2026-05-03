use picoview::{Event, Key, MouseCursor, Point, WindowBuilder};

fn main() {
    WindowBuilder::new(|window| {
        let child = WindowBuilder::new(|window| {
            Box::new(move |event| match event {
                Event::WindowFrame => {}

                Event::WindowClose => {
                    println!("{:?}", event);
                    window.close();
                }

                Event::KeyDown { key, capture } => {
                    if key == Key::Enter {
                        *capture = true;
                    } else if key == Key::Escape {
                        *capture = true;
                        window.close();
                    }
                }

                _ => {
                    println!("child {:?}", event);
                }
            })
        })
        .with_size((200, 200))
        .open_transient(window)
        .expect("failed to open a child window");

        child.wakeup().unwrap();

        Box::new(move |event| match event {
            Event::WindowFrame => {}

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
    .with_title("picoview - transient")
    .with_size((400, 200))
    .with_position((1000, 100))
    .open_blocking()
    .expect("failed to open a window");

    println!("Exiting loop");
}
