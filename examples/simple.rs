use picoview::{Command, Event, MouseCursor, Options, Size, Style, Window};
use std::{thread, time::Duration};

fn main() {
    let window = Window::open(Options {
        parent: None,
        style: Style::Decorated,
        size: Size {
            width: 200.0,
            height: 200.0,
        },
        position: None,
        handler: Box::new(move |event| {
            if !matches!(event, Event::Frame) {
                println!("{:?}", event);
            }
            picoview::EventResponse::Ignored
        }),
    })
    .unwrap();

    // let window2 = Window::open(
    //     Options {
    //         parent: Some(window.raw_window_handle()),
    //         decoration: Decoration::Dock,
    //     },
    //     move |window, event| {
    //         if !matches!(event, Event::Frame) {
    //             println!("WINDOW 2 {:?}", event);
    //         }
    //         picoview::EventResponse::Ignored
    //     },
    // )
    // .unwrap();

    // window2.set_size(Size {
    //     width: 100.0,
    //     height: 100.0,
    // });
    // window2.set_position(Point { x: 0.0, y: 0.0 });
    // window2.set_visible(true);
    // window2.set_cursor_icon(picoview::MouseCursor::Default);

    window.post(Command::SetKeyboardInput(true));
    window.post(Command::SetCursorIcon(MouseCursor::Move));

    thread::sleep(Duration::from_millis(20000));

    window.post(Command::Close);

    thread::sleep(Duration::from_millis(2000));
}
