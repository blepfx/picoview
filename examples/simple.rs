use picoview::{Event, Options, Point, Size, Window};
use std::{
    thread,
    time::{Duration, Instant},
};

fn main() {
    let mut init = Instant::now();
    let windows = Window::open(Options { parent: None }, move |window, event| {
        if !matches!(event, Event::Frame) {
            println!("{:?}", event);
        } else {
            let du = init.elapsed();
            init = Instant::now();
            println!("{:?}", 1.0 / du.as_secs_f64());
        }

        picoview::EventResponse::Ignored
    })
    .unwrap();

    windows.set_title("waow".into());
    windows.set_size(Size {
        width: 600.0,
        height: 200.0,
    });
    windows.set_position(Point {
        x: 1000.0,
        y: 100.0,
    });
    windows.set_visible(true);
    windows.set_cursor_icon(picoview::MouseCursor::NeResize);

    thread::sleep(Duration::from_millis(1000));
    windows.set_visible(false);
    thread::sleep(Duration::from_millis(1000));
    windows.set_visible(true);

    thread::sleep(Duration::from_millis(100000));
}
