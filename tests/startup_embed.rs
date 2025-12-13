use picoview::{Event, WindowBuilder};

#[test]
fn test_startup_embed() {
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
