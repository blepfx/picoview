use picoview::{Event, WindowBuilder};
use std::{thread::sleep, time::Duration};

const WIDTH: u32 = 512;
const HEIGHT: u32 = 256;
const X: u32 = 100;
const Y: u32 = 200;

#[test]
fn test_startup() {
    WindowBuilder::new(|window| {
        Box::new(move |event| {
            if let Event::WindowFrame { .. } = event {
                sleep(Duration::from_millis(100));
                window.close();
            }
        })
    })
    .with_size((WIDTH, HEIGHT))
    .with_position((X, Y))
    .with_visible(true)
    .with_title("picoview test - startup")
    .open_blocking()
    .unwrap();
}
