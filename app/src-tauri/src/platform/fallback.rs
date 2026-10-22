use std::path::PathBuf;
use tauri::{AppHandle, PhysicalPosition, Position, WebviewWindow};

pub fn prepare(_win: &WebviewWindow) {}

pub fn watch_drags(_app: AppHandle) {}

pub fn present(win: &WebviewWindow, _top_left: Position, _focus: bool) {
    let _ = win.show();
    let _ = win.set_focus();
}

pub fn cursor() -> Position {
    PhysicalPosition::new(400, 300).into()
}

pub fn selected_files() -> Vec<PathBuf> {
    Vec::new()
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
