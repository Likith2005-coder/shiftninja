use super::{dbus, fallback};
use gtk::prelude::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalPosition, PhysicalPosition, Position, WebviewWindow};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, KeyButMask, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::{CURRENT_TIME, NONE};

const DRAG_THRESHOLD: i32 = 6;

fn on_x11() -> bool {
    match std::env::var("XDG_SESSION_TYPE") {
        Ok(t) => t == "x11",
        Err(_) => std::env::var_os("DISPLAY").is_some() && std::env::var_os("WAYLAND_DISPLAY").is_none(),
    }
}

struct X {
    conn: RustConnection,
    root: Window,
    win: Window,
    prop: Atom,
}

impl X {
    fn open() -> Option<X> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots[screen].root;
        let win = conn.generate_id().ok()?;
        conn.create_window(0, win, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new())
            .ok()?;
        let mut x = X { conn, root, win, prop: NONE };
        x.prop = x.atom("SHIFTNINJA_SELECTION")?;
        x.conn.flush().ok()?;
        Some(x)
    }

    fn atom(&self, name: &str) -> Option<Atom> {
        Some(self.conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom)
    }

    fn pointer(&self) -> Option<(i32, i32, KeyButMask)> {
        let r = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((r.root_x as i32, r.root_y as i32, r.mask))
    }

    fn owner(&self, selection: Atom) -> Window {
        self.conn
            .get_selection_owner(selection)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.owner)
            .unwrap_or(NONE)
    }

    fn convert(&self, selection: Atom, target: Atom) -> Option<Vec<u8>> {
        self.conn.convert_selection(self.win, selection, target, self.prop, CURRENT_TIME).ok()?;
        self.conn.flush().ok()?;
        let deadline = Instant::now() + Duration::from_millis(300);
        loop {
            match self.conn.poll_for_event().ok()? {
                Some(Event::SelectionNotify(e)) if e.requestor == self.win => {
                    if e.property == NONE {
                        return None;
                    }
                    break;
                }
                Some(_) => continue,
                None if Instant::now() > deadline => return None,
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        let reply = self
            .conn
            .get_property(true, self.win, self.prop, AtomEnum::ANY, 0, u32::MAX / 4)
            .ok()?
            .reply()
            .ok()?;
        Some(reply.value)
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_uris(bytes: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("file://"))
        .map(|rest| rest.strip_prefix("localhost").unwrap_or(rest))
        .map(|p| PathBuf::from(percent_decode(p)))
        .collect()
}

#[derive(Default)]
struct DragState {
    mouse_down: bool,
    start: (i32, i32),
    owner: Window,
    stamp: Option<Vec<u8>>,
    moved: bool,
    shown: bool,
    shift: bool,
}

fn handle(app: &AppHandle, request: dbus::Request) {
    match request {
        dbus::Request::Show { x, y, shortcut: false } => {
            let handle = app.clone();
            let at: Position = LogicalPosition::new(x, y).into();
            let _ = app.run_on_main_thread(move || crate::show_wheel(&handle, Vec::new(), at, "drag"));
        }
        dbus::Request::Show { x, y, shortcut: true } => {
            let app = app.clone();
            std::thread::spawn(move || {
                let paths = selected_files();
                let handle = app.clone();
                let at: Position = LogicalPosition::new(x, y).into();
                let _ = app.run_on_main_thread(move || crate::show_wheel(&handle, paths, at, "shortcut"));
            });
        }
        dbus::Request::Cycle => {
            let _ = app.emit("wheel:cycle", ());
        }
        dbus::Request::MouseUp => {
            let _ = app.emit("wheel:mouseup", ());
        }
    }
}

pub fn watch_drags(app: AppHandle) {
    let for_dbus = app.clone();
    if let Err(e) = dbus::serve(move |request| handle(&for_dbus, request)) {
        eprintln!("ShiftNinja: D-Bus service unavailable ({e}); the GNOME extension can't reach the app");
    }
    if !on_x11() {
        return fallback::watch_drags(app);
    }
    std::thread::spawn(move || {
        let Some(x) = X::open() else { return };
        let (Some(xdnd), Some(uris), Some(timestamp)) =
            (x.atom("XdndSelection"), x.atom("text/uri-list"), x.atom("TIMESTAMP"))
        else {
            return;
        };
        let mut st = DragState::default();
        loop {
            std::thread::sleep(Duration::from_millis(15));
            let Some((px, py, mask)) = x.pointer() else { continue };
            let down = mask.contains(KeyButMask::BUTTON1);
            let shift = mask.contains(KeyButMask::SHIFT);

            if down && !st.mouse_down {
                let owner = x.owner(xdnd);
                let stamp = (owner != NONE).then(|| x.convert(xdnd, timestamp)).flatten();
                st = DragState { mouse_down: true, start: (px, py), owner, stamp, shift, ..Default::default() };
                continue;
            }
            if !down && st.mouse_down {
                if st.shown {
                    let _ = app.emit("wheel:mouseup", ());
                }
                st = DragState::default();
                continue;
            }
            if st.mouse_down && !st.moved {
                st.moved = (px - st.start.0).abs() > DRAG_THRESHOLD || (py - st.start.1).abs() > DRAG_THRESHOLD;
            }
            if st.moved && shift && !st.shift {
                if st.shown {
                    let _ = app.emit("wheel:cycle", ());
                } else {
                    let owner = x.owner(xdnd);
                    let fresh = owner != NONE
                        && (owner != st.owner || x.convert(xdnd, timestamp) != st.stamp);
                    let paths = if fresh { x.convert(xdnd, uris).map(|b| parse_uris(&b)).unwrap_or_default() } else { Vec::new() };
                    if !paths.is_empty() {
                        st.shown = true;
                        let handle = app.clone();
                        let at: Position = PhysicalPosition::new(px, py).into();
                        let _ = app.run_on_main_thread(move || crate::show_wheel(&handle, paths, at, "drag"));
                    }
                }
            }
            st.shift = shift;
        }
    });
}

pub fn prepare(win: &WebviewWindow) {
    fallback::prepare(win);
}

pub fn present(win: &WebviewWindow, _top_left: Position, focus: bool) {
    let _ = win.show();
    if focus {
        let _ = win.set_focus();
    }
}

pub fn cursor() -> Position {
    if on_x11() {
        if let Some((x, y, _)) = X::open().and_then(|x| x.pointer()) {
            return PhysicalPosition::new(x, y).into();
        }
    }
    fallback::cursor()
}

pub fn selected_files() -> Vec<PathBuf> {
    let (tx, rx) = std::sync::mpsc::channel();
    gtk::glib::MainContext::default().invoke(move || {
        let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
        let uris: Vec<String> = clipboard.wait_for_uris().iter().map(|u| u.to_string()).collect();
        let _ = tx.send(parse_uris(uris.join("\n").as_bytes()));
    });
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
}

pub fn native_glass() -> bool {
    false
}

pub fn text_scale() -> f64 {
    1.0
}

pub fn park(win: &WebviewWindow) {
    let _ = win.hide();
}

pub fn editor_glass(_win: &WebviewWindow) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_uri_lists() {
        let list = b"# comment\nfile:///home/me/My%20File.pdf\r\nfile://localhost/tmp/a.png\nhttps://x.y\n";
        assert_eq!(parse_uris(list), vec![PathBuf::from("/home/me/My File.pdf"), PathBuf::from("/tmp/a.png")]);
        assert_eq!(parse_uris(b"copy\nfile:///a/b.jpg"), vec![PathBuf::from("/a/b.jpg")]);
    }
}
