pub const BUS_NAME: &str = "io.github.likith2005coder.ShiftNinja";
pub const OBJECT_PATH: &str = "/io.github.likith2005coder.ShiftNinja";

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Show { x: f64, y: f64, shortcut: bool },
    Cycle,
    MouseUp,
}

struct Service {
    on: Box<dyn Fn(Request) + Send + Sync>,
}

#[zbus::interface(name = "io.github.likith2005coder.ShiftNinja")]
impl Service {
    fn show(&self, x: f64, y: f64, mode: &str) {
        (self.on)(Request::Show { x, y, shortcut: mode == "shortcut" });
    }

    fn cycle(&self) {
        (self.on)(Request::Cycle);
    }

    fn mouse_up(&self) {
        (self.on)(Request::MouseUp);
    }
}

pub fn serve(on: impl Fn(Request) + Send + Sync + 'static) -> zbus::Result<()> {
    let connection = zbus::blocking::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, Service { on: Box::new(on) })?
        .build()?;
    std::mem::forget(connection);
    Ok(())
}
