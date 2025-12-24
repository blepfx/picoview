use picoview::{Event, WindowBuilder};
use std::{thread::sleep, time::Duration};

#[test]
fn test_startup_blocking() {
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

#[test]
fn test_startup_transient() {
    WindowBuilder::new(|window| {
        let mut frames = 0;

        Box::new(move |event| {
            if let Event::WindowFrame { .. } = event {
                if frames == 0 {
                    WindowBuilder::new(|window| {
                        Box::new(move |event| {
                            if let Event::WindowFrame { .. } = event {
                                window.close();
                            }
                        })
                    })
                    .with_size((256, 256))
                    .open_transient(window)
                    .unwrap();

                    WindowBuilder::new(|_| Box::new(move |_| {}))
                        .with_position((256, 0))
                        .with_size((256, 256))
                        .open_transient(window)
                        .unwrap();
                }

                if frames > 100 {
                    window.close();
                }

                frames += 1;
            }
        })
    })
    .with_size((512, 256))
    .with_position((100, 200))
    .with_visible(true)
    .with_title("picoview test - embed")
    .open_blocking()
    .unwrap();
}

#[test]
fn test_startup_embedded() {
    WindowBuilder::new(|window| {
        let mut frames = 0;

        Box::new(move |event| {
            if let Event::WindowFrame { .. } = event {
                if frames == 0 {
                    WindowBuilder::new(|window| {
                        Box::new(move |event| {
                            if let Event::WindowFrame { .. } = event {
                                window.close();
                            }
                        })
                    })
                    .with_size((256, 256))
                    .open_embedded(window)
                    .unwrap();

                    WindowBuilder::new(|_| Box::new(move |_| {}))
                        .with_position((256, 0))
                        .with_size((256, 256))
                        .open_embedded(window)
                        .unwrap();
                }

                if frames > 10 {
                    window.close();
                }

                frames += 1;
            }
        })
    })
    .with_size((512, 256))
    .with_position((100, 200))
    .with_visible(true)
    .with_title("picoview test - embed")
    .open_blocking()
    .unwrap();
}
