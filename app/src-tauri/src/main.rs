#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

macro_rules! debug {
    ($($arg:tt)*) => {
        if std::env::var_os("SHIFTNINJA_DEBUG").is_some() {
            eprintln!("[shiftninja] {}", format!($($arg)*));
        }
    };
}
#[allow(unused_imports)]
pub(crate) use debug;

mod platform;
mod settings;

use serde::Serialize;
use std::path::PathBuf;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use base64::Engine as _;
use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, PhysicalPosition, PhysicalSize, Position, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[cfg(target_os = "macos")]
pub(crate) const SHORTCUT: (Modifiers, &str) = (Modifiers::ALT, "⌥ Space");
#[cfg(not(target_os = "macos"))]
pub(crate) const SHORTCUT: (Modifiers, &str) = (Modifiers::CONTROL.union(Modifiers::ALT), "Ctrl + Alt + Space");

pub(crate) const WHEEL_SIZE: f64 = 320.0;

#[derive(Clone, Serialize)]
struct ShowPayload {
    paths: Vec<String>,
    actions: Vec<engine::ActionInfo>,
    size: u64,
    mode: &'static str,
    drop_in_pixels: bool,
    native_glass: bool,
}

fn scale_at(app: &AppHandle, x: f64, y: f64) -> f64 {
    let monitors = app.available_monitors().unwrap_or_default();
    monitors
        .iter()
        .find(|m| {
            let (p, s) = (m.position(), m.size());
            x >= p.x as f64 && y >= p.y as f64 && x < (p.x + s.width as i32) as f64 && y < (p.y + s.height as i32) as f64
        })
        .map(|m| m.scale_factor())
        .unwrap_or(1.0)
}

pub fn show_wheel(app: &AppHandle, paths: Vec<PathBuf>, at: Position, mode: &'static str) {
    let Some(win) = app.get_webview_window("wheel") else {
        debug!("show_wheel: no wheel window");
        return;
    };
    debug!("show_wheel: {} file(s), mode {mode}, at {at:?}", paths.len());
    let payload = ShowPayload {
        actions: engine::actions_for(&paths),
        size: engine::total_size(&paths),
        paths: paths.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        mode,
        drop_in_pixels: cfg!(target_os = "windows"),
        native_glass: platform::native_glass(),
    };
    let half = WHEEL_SIZE / 2.0;
    let top_left: Position = match at {
        Position::Logical(p) => LogicalPosition::new(p.x - half, p.y - half).into(),
        Position::Physical(p) => {
            let (x, y) = (p.x as f64, p.y as f64);
            let scale = scale_at(app, x, y) * platform::text_scale();
            // The wheel waits off-screen between uses, where Windows sizes it for 100%. Size it for the display under
            // the cursor every time, or the wheel overflows its window on scaled displays.
            let side = (WHEEL_SIZE * scale).round() as u32;
            let _ = win.set_size(PhysicalSize::new(side, side));
            let half = half * scale;
            PhysicalPosition::new((x - half) as i32, (y - half) as i32).into()
        }
    };
    let _ = win.set_position(top_left);
    let _ = win.emit("wheel:show", payload);
    platform::present(&win, top_left, mode == "shortcut");
}

#[derive(Serialize)]
struct Slices {
    actions: Vec<engine::ActionInfo>,
    size: u64,
}

#[tauri::command]
fn actions_for(paths: Vec<String>) -> Slices {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    Slices { actions: engine::actions_for(&paths), size: engine::total_size(&paths) }
}

#[tauri::command]
async fn run_action(id: String, paths: Vec<String>) -> Result<Vec<String>, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    tauri::async_runtime::spawn_blocking(move || engine::run(&id, &paths))
        .await
        .map_err(|e| e.to_string())?
        .map(|out| out.iter().map(|p| p.to_string_lossy().into_owned()).collect())
        .map_err(|e| e.to_string())
}

// Async on purpose: building a window inside a synchronous command deadlocks on Windows, which left editors
// blank and the wheel stuck waiting.
#[tauri::command]
async fn open_panel(app: AppHandle, panel: String, path: String) -> Result<(), String> {
    let title = match panel.as_str() {
        "crop" => "Crop image",
        "edit" => "Edit image",
        "annotate" => "Annotate",
        "addbg" => "Add background",
        "redact" => "Redact",
        _ => return Err(format!("unknown panel {panel}")),
    };
    static N: AtomicU32 = AtomicU32::new(0);
    let label = format!("editor-{}", N.fetch_add(1, Ordering::Relaxed));
    let glass = cfg!(any(target_os = "macos", target_os = "windows"));
    let init = format!(
        "window.__SHIFTNINJA_EDIT__ = {};",
        serde_json::json!({ "mode": panel, "path": path, "glass": glass })
    );
    let win = WebviewWindowBuilder::new(&app, label, WebviewUrl::App("editor.html".into()))
        .title(title)
        .inner_size(960.0, 680.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .transparent(glass)
        .initialization_script(&init)
        .build()
        .map_err(|e| e.to_string())?;
    if glass {
        platform::editor_glass(&win);
    }
    let _ = win.set_focus();
    Ok(())
}

#[derive(Serialize)]
struct EditImage {
    data_url: String,
    width: u32,
    height: u32,
    ext: &'static str,
    name: String,
}

#[tauri::command]
async fn load_for_edit(path: String) -> Result<EditImage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        let src = engine::load_for_edit(&path).map_err(|e| e.to_string())?;
        Ok(EditImage {
            data_url: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&src.png)),
            width: src.width,
            height: src.height,
            ext: src.out_ext,
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_edit(path: String, suffix: String, ext: String, data: String) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || engine::save_edit(&PathBuf::from(path), &suffix, &ext, &bytes))
        .await
        .map_err(|e| e.to_string())?
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_wheel(app: AppHandle) {
    if let Some(win) = app.get_webview_window("wheel") {
        platform::park(&win);
    }
}

fn open_from_selection(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let paths = platform::selected_files();
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            show_wheel(&handle, paths, platform::cursor(), "shortcut");
        });
    });
}

fn tray_menu<M: Manager<tauri::Wry>>(app: &M) -> tauri::Result<Menu<tauri::Wry>> {
    let card = MenuItem::with_id(app, "card", "How to use && settings…", true, None::<&str>)?;
    let hint = MenuItem::with_id(app, "hint", "Drag a file and press Shift", false, None::<&str>)?;
    let label = format!("Or select a file and press {}", SHORTCUT.1);
    let shortcut = MenuItem::with_id(app, "shortcut", label, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", format!("Quit {}", settings::NAME), true, None::<&str>)?;
    Menu::with_items(
        app,
        &[&card, &PredefinedMenuItem::separator(app)?, &hint, &shortcut, &PredefinedMenuItem::separator(app)?, &quit],
    )
}

fn tray(app: &tauri::App) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip(settings::NAME)
        .menu(&tray_menu(app)?)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                settings::show_card(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "card" => settings::show_card(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        // First, so a second launch just brings the running copy forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| settings::show_card(app)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![settings::HIDDEN_ARG]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        open_from_selection(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            actions_for,
            run_action,
            hide_wheel,
            open_panel,
            load_for_edit,
            save_edit,
            settings::app_info,
            settings::autostart_enabled,
            settings::set_autostart,
        ])
        .setup(|app| {
            if std::env::var_os("SHIFTNINJA_PDFIUM").is_none() {
                if let Ok(dir) = app.path().resource_dir() {
                    std::env::set_var("SHIFTNINJA_PDFIUM", dir.join("lib"));
                }
            }
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray(app)?;
            let shortcut = Shortcut::new(Some(SHORTCUT.0), Code::Space);
            if let Err(e) = app.global_shortcut().register(shortcut) {
                eprintln!("{}: couldn't register {}: {e}", settings::NAME, SHORTCUT.1);
            }
            if let Some(win) = app.get_webview_window("wheel") {
                platform::prepare(&win);
            }
            platform::watch_drags(app.handle().clone());
            settings::on_launch(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ShiftNinja");
}
