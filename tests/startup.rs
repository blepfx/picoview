use picoview::{Event, WindowBuilder};
use std::thread::sleep;
use std::time::Duration;

/// Because some OSes require the windows to be created on the main-thread
/// we have to run the tests with `harness = false`.
fn main() {
    test_startup_blocking();
    sleep(Duration::from_millis(100));
    test_startup_blocking_undecorated();
    sleep(Duration::from_millis(100));
    test_startup_transient();
    sleep(Duration::from_millis(100));
    test_startup_embedded();
}

fn test_startup_blocking() {
    WindowBuilder::new(|window| {
        window.set_title("picoview test - blocking");
        window.set_size((512, 256));
        window.set_position((100, 200));
        window.set_visible(true);

        Box::new(move |event| {
            if let Event::WindowFrame { .. } = event {
                sleep(Duration::from_millis(500));
                window.close();
            }
        })
    })
    .open_blocking()
    .unwrap();
}

fn test_startup_blocking_undecorated() {
    WindowBuilder::new(|window| {
        window.set_title("picoview test - blocking undecorated");
        window.set_size((512, 256));
        window.set_position((100, 200));
        window.set_visible(true);

        Box::new(move |event| {
            if let Event::WindowFrame { .. } = event {
                sleep(Duration::from_millis(1000));
                window.close();
            }
        })
    })
    .with_decorations(false)
    .open_blocking()
    .unwrap();
}

fn test_startup_transient() {
    WindowBuilder::new(|window| {
        window.set_title("picoview test - transient");
        window.set_size((512, 256));
        window.set_position((100, 200));
        window.set_visible(true);

        let mut frames = 0;
        Box::new(move |event| {
            if let Event::WindowFrame { .. } = event {
                if frames == 0 {
                    WindowBuilder::new(|window| {
                        window.set_title("picoview test - transient child");
                        window.set_size((256, 256));
                        window.set_position((256, 0));
                        window.set_visible(true);

                        Box::new(move |_| {})
                    })
                    .open_transient(window)
                    .unwrap();
                }

                if frames > 10 {
                    window.close();
                }

                frames += 1;
            }
        })
    })
    .open_blocking()
    .unwrap();
}

fn test_startup_embedded() {
    WindowBuilder::new(|window| {
        window.set_title("picoview test - embed");
        window.set_size((512, 256));
        window.set_position((100, 200));
        window.set_visible(true);

        let mut frames = 0;
        Box::new(move |event| {
            if let Event::WindowFrame { .. } = event {
                if frames == 0 {
                    WindowBuilder::new(|window| {
                        window.set_title("picoview test - embed child (self close)");
                        window.set_size((256, 256));
                        window.set_visible(true);

                        Box::new(move |event| {
                            if let Event::WindowFrame { .. } = event {
                                window.close();
                            }
                        })
                    })
                    .open_embedded(window)
                    .unwrap();

                    WindowBuilder::new(|window| {
                        window.set_title("picoview test - embed child (no close)");
                        window.set_position((256, 0));
                        window.set_size((256, 256));
                        window.set_visible(true);

                        Box::new(move |_| {})
                    })
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
    .open_blocking()
    .unwrap();
}
