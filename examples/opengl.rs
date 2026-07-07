use picoview::*;
use std::mem::transmute;

fn main() {
    WindowBuilder::new(|window| {
        window.set_max_size((1000, 1000));
        window.set_size((200, 200));
        window.set_title("OpenGL Example");
        window.set_visible(true);

        Ok(Box::new(Handler {
            window,
            opengl: window.opengl()?,
            scale: window.scale(),
            size: Size {
                width: 200,
                height: 200,
            },
        }))
    })
    .with_opengl(GlConfig {
        version: GlVersion::Compat(2, 1),
        ..Default::default()
    })
    .with_transparency(true)
    .open_blocking()
    .expect("failed to open a window");
}

struct Handler<'a> {
    window: Window<'a>,
    opengl: GlContext<'a>,
    size: Size,
    scale: f64,
}

impl<'a> WindowHandler for Handler<'a> {
    fn frame(&mut self) {
        // we just rawdogging opengl here lol
        let gl = self.opengl;
        let clear_color: unsafe extern "system" fn(f32, f32, f32, f32) =
            unsafe { transmute(gl.get_proc_address(c"glClearColor")) };
        let clear: unsafe extern "system" fn(i32) =
            unsafe { transmute(gl.get_proc_address(c"glClear")) };
        let begin: unsafe extern "system" fn(i32) =
            unsafe { transmute(gl.get_proc_address(c"glBegin")) };
        let end: unsafe extern "system" fn() = unsafe { transmute(gl.get_proc_address(c"glEnd")) };
        let vertex: unsafe extern "system" fn(f32, f32, f32) =
            unsafe { transmute(gl.get_proc_address(c"glVertex3f")) };
        let viewport: unsafe extern "system" fn(i32, i32, i32, i32) =
            unsafe { transmute(gl.get_proc_address(c"glViewport")) };
        let color: unsafe extern "system" fn(f32, f32, f32, f32) =
            unsafe { transmute(gl.get_proc_address(c"glColor4f")) };

        gl.make_current(true).unwrap();

        unsafe {
            (clear_color)(0.25, 0.25, 0.25, 0.5);
            (clear)(0x00004000);
            (viewport)(0, 0, self.size.width as i32, self.size.height as i32);

            let draw_line = |x1: f32, y1: f32, x2: f32, y2: f32| {
                let x1 = (x1 / self.size.width as f32) * 2.0 - 1.0;
                let y1 = 1.0 - (y1 / self.size.height as f32) * 2.0;
                let x2 = (x2 / self.size.width as f32) * 2.0 - 1.0;
                let y2 = 1.0 - (y2 / self.size.height as f32) * 2.0;

                (vertex)(x1, y1, 0.0);
                (vertex)(x2, y2, 0.0);
            };

            (begin)(0x0001); // GL_LINES

            (color)(0.5, 0.5, 0.5, 0.5);
            for i in (0..1000).step_by(25) {
                draw_line(i as f32, 0.0, i as f32, self.size.height as f32);
                draw_line(0.0, i as f32, self.size.width as f32, i as f32);
            }

            (color)(1.0, 1.0, 1.0, 1.0);
            for i in (0..1000).step_by(100) {
                draw_line(i as f32, 0.0, i as f32, self.size.height as f32);
                draw_line(0.0, i as f32, self.size.width as f32, i as f32);
            }

            (color)(1.0, 0.0, 0.0, 0.5);
            for i in (0..1000).step_by((100.0 * self.scale) as usize) {
                draw_line(i as f32, 0.0, i as f32, self.size.height as f32);
                draw_line(0.0, i as f32, self.size.width as f32, i as f32);
            }

            (end)();
        }

        gl.swap_buffers().unwrap();
        gl.make_current(false).unwrap();
    }

    fn mouse_move(&mut self, point: Point) {
        println!("mouse_move({:?})", point);

        if point.x < -10.0 {
            self.window.set_cursor_position((100.0, 100.0));
        }
    }

    fn close_requested(&mut self) {
        self.window.close();
    }

    fn wakeup(&mut self) {}

    fn damage(&mut self, rect: Rect) {
        println!("damage({rect:?})");
    }

    fn focus_changed(&mut self, focus: bool) {
        println!("focus_changed({focus})");
    }

    fn position_changed(&mut self, position: Point) {
        println!("position_changed({position:?})");
    }

    fn visibility_changed(&mut self, state: WindowVisibility) {
        println!("visibility_changed({state:?})");
    }

    fn size_changed(&mut self, size: Size) {
        println!("size_changed({size:?})");
        self.size = size;
    }

    fn scale_changed(&mut self, scale: f64) {
        println!("scale_changed({scale})");
        self.scale = scale;
    }

    fn mouse_leave(&mut self) {
        println!("mouse_leave()");
    }

    fn mouse_press(&mut self, button: MouseButton, pressed: bool) {
        println!("mouse_press({button:?}, {pressed})");

        if button == MouseButton::Right && pressed {
            self.window.set_size((500, 500));
        }
    }

    fn mouse_scroll(&mut self, x: f64, y: f64) {
        println!("mouse_scroll({x}, {y})");
    }

    fn gesture_rotate(&mut self, angle: f64) {
        println!("gesture_rotate({angle})");
    }

    fn gesture_zoom(&mut self, scale: f64) {
        println!("gesture_zoom({scale})");
    }

    fn key_modifiers(&mut self, modifiers: Modifiers) {
        println!("key_modifiers({modifiers:?})");
    }

    fn key_press(&mut self, key: Key, pressed: bool) -> bool {
        println!("key_press({key:?}, {pressed})");
        false
    }

    fn drag_enter(&mut self, data: Exchange, point: Point) -> DropEffect {
        println!("drag_enter({data:?}, {point:?})");
        DropEffect::Reject
    }

    fn drag_move(&mut self, point: Point) -> DropEffect {
        println!("drag_move({point:?})");
        DropEffect::Reject
    }

    fn drag_leave(&mut self) {
        println!("drag_leave()");
    }

    fn drag_accept(&mut self) -> DropEffect {
        println!("drag_accept()");
        DropEffect::Reject
    }
}
