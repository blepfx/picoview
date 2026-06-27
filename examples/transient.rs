use picoview::{Key, MouseButton, MouseCursor, Point, Window, WindowBuilder, WindowHandler};

fn main() {
    WindowBuilder::new(|window| {
        window.set_title("picoview test - transient");
        window.set_size((400, 200));
        window.set_position((100, 200));
        window.set_visible(true);

        let child = WindowBuilder::new(|window| {
            window.set_size((200, 200));
            window.set_visible(true);

            Ok(Box::new(Child { window }))
        })
        .open_transient(window)
        .expect("failed to open a child window");

        child.wakeup().unwrap();

        Ok(Box::new(Parent { window }))
    })
    .open_blocking()
    .expect("failed to open a window");

    println!("Exiting loop");
}

struct Child<'a> {
    window: Window<'a>,
}

struct Parent<'a> {
    window: Window<'a>,
}

impl WindowHandler for Parent<'_> {
    fn close_requested(&mut self) {
        self.window.close();
    }

    fn focus_changed(&mut self, focus: bool) {
        println!("parent.focus_changed({focus})");
    }

    fn mouse_move(&mut self, point: Point) {
        if point.x < -10.0 {
            self.window.set_cursor_position((100.0, 100.0));
        }

        if point.x < 10.0 {
            self.window.set_cursor_icon(MouseCursor::Hidden);
        } else {
            self.window.set_cursor_icon(MouseCursor::Default);
        }
    }

    fn key_press(&mut self, key: Key, pressed: bool) -> bool {
        println!("parent.key_press({key:?}, {pressed})");
        false
    }
}

impl WindowHandler for Child<'_> {
    fn close_requested(&mut self) {
        self.window.close();
    }

    fn wakeup(&mut self) {
        println!("child.wakeup()");
    }

    fn focus_changed(&mut self, focus: bool) {
        println!("child.focus_changed({focus})");
    }

    fn mouse_press(&mut self, button: MouseButton, pressed: bool) {
        if button == MouseButton::Right && pressed {
            self.window.set_position((1000, 200));
        }
    }

    fn key_press(&mut self, key: Key, pressed: bool) -> bool {
        println!("child.key_press({key:?}, {pressed})");

        if key == Key::Escape && pressed {
            self.window.close();
            return true;
        }

        false
    }
}
