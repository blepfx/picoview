use picoview::{Event, WindowBuilder};
use std::{thread::sleep, time::Duration};

#[test]
fn test_startup_window() {
    WindowBuilder::new(|window| {
        Box::new(move |event| {
            if let Event::WindowFrame { .. } = event {
                sleep(Duration::from_millis(100));
                window.close();
            }
        })
    })
    .with_size((512, 256))
    .with_position((100, 200))
    .with_visible(true)
    .with_title("picoview test - startup")
    .open_blocking()
    .unwrap();
}
