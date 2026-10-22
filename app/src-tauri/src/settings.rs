//! What makes the wheel a desktop tool: start-with-Windows and the small "how to use & settings" card.
//! The wheel itself stays the only way to use the tools.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt as _;

pub const NAME: &str = "ShiftNinja";
/// Passed by the Windows startup entry so a boot-time launch stays quietly in the tray.
pub const HIDDEN_ARG: &str = "--hidden";

fn data_dir(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join(NAME));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[derive(Serialize)]
pub struct AppInfo {
    name: &'static str,
    version: String,
    shortcut: &'static str,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo { name: NAME, version: app.package_info().version.to_string(), shortcut: crate::SHORTCUT.1 }
}

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    autostart_chosen: bool,
    #[serde(default)]
    welcomed: bool,
}

fn settings_path(app: &AppHandle) -> PathBuf {
    data_dir(app).join("settings.json")
}

fn load_settings(app: &AppHandle) -> Settings {
    std::fs::read(settings_path(app)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_settings(app: &AppHandle, s: &Settings) {
    if let Ok(json) = serde_json::to_vec_pretty(s) {
        let _ = std::fs::write(settings_path(app), json);
    }
}

#[tauri::command]
pub fn autostart_enabled(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, on: bool) -> Result<bool, String> {
    let launcher = app.autolaunch();
    if on { launcher.enable() } else { launcher.disable() }.map_err(|e| e.to_string())?;
    let mut s = load_settings(&app);
    s.autostart_chosen = true;
    save_settings(&app, &s);
    Ok(launcher.is_enabled().unwrap_or(on))
}

/// First launch: start with Windows (so it is back after a reboot) and show how to use it once,
/// since a tray tool is otherwise invisible.
pub fn on_launch(app: &AppHandle) {
    let mut s = load_settings(app);
    let first = !s.welcomed;
    if !s.autostart_chosen {
        let _ = app.autolaunch().enable();
        s.autostart_chosen = true;
    }
    s.welcomed = true;
    save_settings(app, &s);
    if first {
        show_card(app);
    }
}

pub fn show_card(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("card") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let glass = cfg!(any(target_os = "macos", target_os = "windows"));
    let init = format!("window.__SHIFTNINJA__ = {};", serde_json::json!({ "glass": glass }));
    let built = WebviewWindowBuilder::new(app, "card", WebviewUrl::App("card.html".into()))
        .title(NAME)
        .inner_size(440.0, 520.0)
        .resizable(false)
        .maximizable(false)
        .center()
        .transparent(glass)
        .initialization_script(&init)
        .build();
    if let Ok(win) = built {
        if glass {
            crate::platform::editor_glass(&win);
        }
        let _ = win.set_focus();
    }
}
