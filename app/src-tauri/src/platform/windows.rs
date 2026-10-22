use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Position, WebviewWindow};
use windows::core::Interface;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IDispatch, IServiceProvider, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_SHIFT};
use windows::Win32::UI::Shell::{
    IFolderView, IShellBrowser, IShellItemArray, IShellWindows, IWebBrowserApp, SID_STopLevelBrowser,
    ShellWindows, SIGDN_FILESYSPATH, SVGIO_SELECTION, SWC_DESKTOP, SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetCursorPos, GetForegroundWindow, GetSystemMetrics, IsWindowVisible, SetWindowPos,
    ShowWindow, WindowFromPoint, GA_ROOT, HWND_TOPMOST, SM_CXDRAG, SM_CYDRAG, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE,
};

const CSIDL_DESKTOP: i32 = 0;

fn variant_i32(v: i32) -> VARIANT {
    let mut var = VARIANT::default();
    unsafe {
        let inner = &mut *var.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = v;
    }
    var
}

fn key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn cursor_point() -> POINT {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    p
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn is_desktop(root: HWND) -> bool {
    matches!(class_name(root).as_str(), "Progman" | "WorkerW")
}

fn file_window_at(p: POINT) -> Option<HWND> {
    let root = unsafe { GetAncestor(WindowFromPoint(p), GA_ROOT) };
    let class = class_name(root);
    (class == "CabinetWClass" || is_desktop(root)).then_some(root)
}

#[derive(Default)]
struct DragState {
    mouse_down: bool,
    start: (i32, i32),
    source: Option<HWND>,
    dragging: bool,
    shown: bool,
    shift: bool,
    last: (i32, i32),
}

/// The cursor in the wheel page's own 320×320 coordinates, or None if the wheel window is gone.
fn wheel_point(app: &AppHandle, p: POINT) -> Option<serde_json::Value> {
    let win = app.get_webview_window("wheel")?;
    let (pos, size) = (win.outer_position().ok()?, win.outer_size().ok()?);
    let k = crate::WHEEL_SIZE / size.width.max(1) as f64;
    Some(serde_json::json!({ "x": (p.x - pos.x) as f64 * k, "y": (p.y - pos.y) as f64 * k }))
}

pub fn watch_drags(app: AppHandle) {
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let (tx, ty) = unsafe { (GetSystemMetrics(SM_CXDRAG), GetSystemMetrics(SM_CYDRAG)) };
        let mut st = DragState::default();
        loop {
            std::thread::sleep(Duration::from_millis(15));
            let down = key_down(VK_LBUTTON.0);
            let shift = key_down(VK_SHIFT.0);
            let p = cursor_point();

            if down && !st.mouse_down {
                st = DragState { mouse_down: true, start: (p.x, p.y), source: file_window_at(p), shift, ..Default::default() };
                continue;
            }
            if !down && st.mouse_down {
                // WebView2 often never becomes a drop target for drags out of Explorer, so the drop itself is
                // unreliable. The release point is enough: the wheel runs whichever petal it lands on.
                if st.shown {
                    let _ = app.emit("wheel:mouseup", wheel_point(&app, p));
                }
                st = DragState::default();
                continue;
            }
            if st.mouse_down && !st.dragging && st.source.is_some() {
                st.dragging = (p.x - st.start.0).abs() > tx || (p.y - st.start.1).abs() > ty;
            }
            if st.dragging && shift && !st.shift {
                if st.shown {
                    let _ = app.emit("wheel:cycle", ());
                } else if let Some(source) = st.source {
                    let paths = explorer_selection(source);
                    if !paths.is_empty() {
                        st.shown = true;
                        let handle = app.clone();
                        let at: Position = PhysicalPosition::new(p.x, p.y).into();
                        let _ = app.run_on_main_thread(move || crate::show_wheel(&handle, paths, at, "drag"));
                    }
                }
            }
            if st.shown && (p.x, p.y) != st.last {
                st.last = (p.x, p.y);
                if let Some(at) = wheel_point(&app, p) {
                    let _ = app.emit("wheel:pointer", at);
                }
            }
            st.shift = shift;
        }
    });
}

fn explorer_selection(root: HWND) -> Vec<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let Ok(windows) = CoCreateInstance::<_, IShellWindows>(&ShellWindows, None, CLSCTX_ALL) else {
            return Vec::new();
        };
        let dispatch = if is_desktop(root) {
            let mut hwnd = 0i32;
            windows
                .FindWindowSW(&variant_i32(CSIDL_DESKTOP), &VARIANT::default(), SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)
                .ok()
        } else {
            // Windows 11 Explorer tabs all share one top-level window; only the visible tab is the one in front.
            let count = windows.Count().unwrap_or(0);
            let tabs: Vec<IDispatch> = (0..count)
                .filter_map(|i| {
                    let d = windows.Item(&variant_i32(i)).ok()?;
                    let app: IWebBrowserApp = d.cast().ok()?;
                    (app.HWND().ok()?.0 as isize == root.0 as isize).then_some(d)
                })
                .collect();
            tabs.iter().find(|d| tab_visible(d)).or(tabs.first()).cloned()
        };
        dispatch.and_then(|d| selection_of(&d).ok()).unwrap_or_default()
    }
}

unsafe fn tab_visible(dispatch: &IDispatch) -> bool {
    let view = || -> windows::core::Result<HWND> {
        let provider: IServiceProvider = dispatch.cast()?;
        let browser: IShellBrowser = provider.QueryService(&SID_STopLevelBrowser)?;
        browser.QueryActiveShellView()?.GetWindow()
    };
    view().map(|hwnd| IsWindowVisible(hwnd).as_bool()).unwrap_or(false)
}

unsafe fn selection_of(dispatch: &IDispatch) -> windows::core::Result<Vec<PathBuf>> {
    let provider: IServiceProvider = dispatch.cast()?;
    let browser: IShellBrowser = provider.QueryService(&SID_STopLevelBrowser)?;
    let view: IFolderView = browser.QueryActiveShellView()?.cast()?;
    let items: IShellItemArray = view.Items(SVGIO_SELECTION)?;
    let mut paths = Vec::new();
    for i in 0..items.GetCount()? {
        let name = items.GetItemAt(i)?.GetDisplayName(SIGDN_FILESYSPATH)?;
        paths.push(PathBuf::from(name.to_string()?));
        CoTaskMemFree(Some(name.0 as _));
    }
    Ok(paths)
}

fn hwnd_of(win: &WebviewWindow) -> Option<HWND> {
    win.hwnd().ok().map(|h| HWND(h.0 as _))
}

pub fn prepare(win: &WebviewWindow) {
    park(win);
}

pub fn native_glass() -> bool {
    false
}

/// Windows' "Text size" setting (Settings → Accessibility), which WebView2 applies as page zoom on top of display
/// scaling. 1.0 when unset.
pub fn text_scale() -> f64 {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let found = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Accessibility"),
            w!("TextScaleFactor"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as _),
            Some(&mut size),
        )
    };
    if found.is_ok() && (100..=225).contains(&value) { value as f64 / 100.0 } else { 1.0 }
}

const PARKED: i32 = -32000;

/// WebView2 stops painting hidden windows, so a freshly shown wheel flashes blank before it catches up. Keep it
/// shown, far off-screen, so it is already drawn the moment it moves under the cursor.
pub fn park(win: &WebviewWindow) {
    if let Some(hwnd) = hwnd_of(win) {
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), PARKED, PARKED, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    }
}

/// Mica on Windows 11, Acrylic on Windows 10. Fine for editors: they are normal, focused windows.
pub fn editor_glass(win: &WebviewWindow) {
    if window_vibrancy::apply_mica(win, None).is_err() {
        let _ = window_vibrancy::apply_acrylic(win, None);
    }
}

pub fn present(win: &WebviewWindow, top_left: Position, focus: bool) {
    let (x, y) = match top_left {
        Position::Physical(p) => (p.x, p.y),
        Position::Logical(p) => {
            let s = win.scale_factor().unwrap_or(1.0);
            ((p.x * s) as i32, (p.y * s) as i32)
        }
    };
    if let Some(hwnd) = hwnd_of(win) {
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
    if focus {
        let _ = win.show();
        let _ = win.set_focus();
    } else if let Some(hwnd) = hwnd_of(win) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    }
}

pub fn cursor() -> Position {
    let p = cursor_point();
    PhysicalPosition::new(p.x, p.y).into()
}

pub fn selected_files() -> Vec<PathBuf> {
    let root = unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) };
    explorer_selection(root)
}
