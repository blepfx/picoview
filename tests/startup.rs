use picoview::{Window, WindowBuilder, WindowHandler};
use std::thread::sleep;
use std::time::{Duration, Instant};

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
    sleep(Duration::from_millis(100));
    test_startup_error();
}

fn test_startup_blocking() {
    struct Handler<'a> {
        window: Window<'a>,
        instant: Instant,
    }

    impl WindowHandler for Handler<'_> {
        fn frame(&mut self) {
            if self.instant.elapsed() > Duration::from_millis(500) {
                self.window.close();
            }
        }
    }

    WindowBuilder::new(|window| {
        window.set_title("picoview test - blocking");
        window.set_size((512, 256));
        window.set_position((100, 200));
        window.set_visible(true);

        Ok(Box::new(Handler {
            window,
            instant: Instant::now(),
        }))
    })
    .open_blocking()
    .unwrap();
}

fn test_startup_blocking_undecorated() {
    struct Handler<'a> {
        window: Window<'a>,
        instant: Instant,
    }

    impl WindowHandler for Handler<'_> {
        fn frame(&mut self) {
            if self.instant.elapsed() > Duration::from_millis(500) {
                self.window.close();
            }
        }
    }

    WindowBuilder::new(|window| {
        window.set_title("picoview test - blocking undecorated");
        window.set_size((512, 256));
        window.set_position((100, 200));
        window.set_decorations(false);
        window.set_visible(true);

        Ok(Box::new(Handler {
            window,
            instant: Instant::now(),
        }))
    })
    .open_blocking()
    .unwrap();
}

fn test_startup_transient() {
    struct Handler<'a> {
        window: Window<'a>,
        frames: usize,
    }

    impl WindowHandler for Handler<'_> {
        fn frame(&mut self) {
            if self.frames == 0 {
                WindowBuilder::new(|window| {
                    window.set_title("picoview test - transient child");
                    window.set_size((256, 256));
                    window.set_position((256, 0));
                    window.set_visible(true);

                    Ok(Box::new(()))
                })
                .open_transient(self.window)
                .unwrap();
            }

            if self.frames > 10 {
                self.window.close();
            }

            self.frames += 1;
        }
    }

    WindowBuilder::new(|window| {
        window.set_title("picoview test - transient");
        window.set_size((512, 256));
        window.set_position((100, 200));
        window.set_visible(true);

        Ok(Box::new(Handler { window, frames: 0 }))
    })
    .open_blocking()
    .unwrap();
}

fn test_startup_embedded() {
    struct Handler<'a> {
        window: Window<'a>,
        frames: usize,
    }

    impl WindowHandler for Handler<'_> {
        fn frame(&mut self) {
            if self.frames == 0 {
                WindowBuilder::new(|window| {
                    struct Handler<'a> {
                        window: Window<'a>,
                    }

                    impl WindowHandler for Handler<'_> {
                        fn frame(&mut self) {
                            self.window.close();
                        }
                    }

                    window.set_title("picoview test - embed child (self close)");
                    window.set_size((256, 256));
                    window.set_visible(true);

                    Ok(Box::new(Handler { window }))
                })
                .open_embedded(self.window)
                .unwrap();

                WindowBuilder::new(|window| {
                    window.set_title("picoview test - embed child (no close)");
                    window.set_position((256, 0));
                    window.set_size((256, 256));
                    window.set_visible(true);

                    Ok(Box::new(()))
                })
                .open_embedded(self.window)
                .unwrap();
            }

            if self.frames > 10 {
                self.window.close();
            }

            self.frames += 1;
        }
    }

    WindowBuilder::new(|window| {
        window.set_title("picoview test - embed");
        window.set_size((512, 256));
        window.set_position((100, 200));
        window.set_visible(true);

        Ok(Box::new(Handler { window, frames: 0 }))
    })
    .open_blocking()
    .unwrap();
}

fn test_startup_error() {
    let err = WindowBuilder::new(|window| {
        window.set_title("picoview test - error");
        window.set_size((512, 256));
        window.set_position((100, 200));
        window.set_visible(true);

        Err("test error".into())
    })
    .open_blocking()
    .unwrap_err();

    assert_eq!(err.to_string(), "test error");
}
