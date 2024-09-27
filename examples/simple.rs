use picoview::{Decoration, Event, Options, Point, Size, Window};
use std::{thread, time::Duration};

fn main() {
    let window = Window::open(
        Options {
            parent: None,
            decoration: Decoration::Dock,
        },
        move |window, event| {
            if !matches!(event, Event::Frame) {
                println!("{:?}", event);
            }
            picoview::EventResponse::Ignored
        },
    )
    .unwrap();

    window.set_title("waow".into());
    window.set_size(Size {
        width: 600.0,
        height: 200.0,
    });
    window.set_position(Point {
        x: 1000.0,
        y: 100.0,
    });
    window.set_visible(true);
    window.set_cursor_icon(picoview::MouseCursor::NeResize);

    thread::sleep(Duration::from_millis(1000));
    window.set_visible(false);
    thread::sleep(Duration::from_millis(1000));
    window.set_visible(true);

    let window2 = Window::open(
        Options {
            parent: Some(window.raw_window_handle()),
            decoration: Decoration::Dock,
        },
        move |window, event| {
            if !matches!(event, Event::Frame) {
                println!("WINDOW 2 {:?}", event);
            }
            picoview::EventResponse::Ignored
        },
    )
    .unwrap();

    window2.set_size(Size {
        width: 100.0,
        height: 100.0,
    });
    window2.set_position(Point { x: 0.0, y: 0.0 });
    window2.set_visible(true);
    window2.set_cursor_icon(picoview::MouseCursor::Default);

    thread::sleep(Duration::from_millis(100000));
}
