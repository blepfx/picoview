use picoview::{Key, MouseCursor, Point, Window, WindowBuilder, WindowHandler};

fn main() {
    WindowBuilder::new(|window| {
        window.set_title("picoview test - embed");
        window.set_size((400, 200));
        window.set_position((1000, 100));
        window.set_visible(true);

        WindowBuilder::new(|window| {
            window.set_size((200, 200));
            window.set_visible(true);

            Box::new(Child {
                window,
                name: "left",
                cursor: MouseCursor::Crosshair,
            })
        })
        .open_embedded(window)
        .expect("failed to open a child window");

        WindowBuilder::new(|window| {
            window.set_size((200, 200));
            window.set_position((200, 0));
            window.set_visible(true);

            Box::new(Child {
                window,
                name: "right",
                cursor: MouseCursor::NotAllowed,
            })
        })
        .open_embedded(window)
        .expect("failed to open a child window");

        Box::new(Parent { window })
    })
    .open_blocking()
    .expect("failed to open a window");

    println!("Exiting loop");
}

struct Parent<'a> {
    window: Window<'a>,
}

struct Child<'a> {
    window: Window<'a>,
    name: &'static str,
    cursor: MouseCursor,
}

impl WindowHandler for Parent<'_> {
    fn close(&mut self) {
        self.window.close();
    }

    fn focus_changed(&mut self, focus: bool) {
        println!("parent.focus_changed({focus})");
    }

    fn mouse_move(&mut self, point: Point) {
        println!("parent.mouse_move({:?})", point);
        self.window.set_cursor_icon(MouseCursor::Default);
    }

    fn mouse_leave(&mut self) {
        println!("parent.mouse_leave()");
    }

    fn key_press(&mut self, key: Key, pressed: bool) -> bool {
        println!("parent.key_press({key:?}, {pressed})");
        false
    }
}

impl WindowHandler for Child<'_> {
    fn close(&mut self) {
        self.window.close();
    }

    fn focus_changed(&mut self, focus: bool) {
        println!("{}.focus_changed({focus})", self.name);
    }

    fn mouse_press(&mut self, button: picoview::MouseButton, pressed: bool) {
        println!("{}.mouse_press({button:?}, {pressed})", self.name);
    }

    fn mouse_move(&mut self, point: Point) {
        println!("{}.mouse_move({:?})", self.name, point);
        self.window.set_cursor_icon(self.cursor);
    }

    fn mouse_leave(&mut self) {
        println!("{}.mouse_leave()", self.name);
    }

    fn key_press(&mut self, key: Key, pressed: bool) -> bool {
        let capture = key == Key::Enter || key == Key::Escape;
        println!("{}.key_press({key:?}, {pressed}) -> {}", self.name, capture);
        capture
    }
}
