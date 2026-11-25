use crate::AppState;
use std::sync::atomic::Ordering;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn show_main_window(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.window_visible.store(true, Ordering::Release);
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn minimize_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn toggle_maximize_window(window: tauri::WebviewWindow) -> Result<bool, String> {
    let maximized = window.is_maximized().map_err(|e| e.to_string())?;
    if maximized {
        window.unmaximize().map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        window.maximize().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
#[specta::specta]
pub fn close_main_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn quit_app(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.window_visible.store(false, Ordering::Release);
    if let Ok(mut guard) = state.telemetry_cancel.lock() {
        if let Some(flag) = guard.take() {
            flag.store(true, Ordering::Release);
        }
    }
    state.engine.persist_snapshot();
    app.exit(0);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn is_window_maximized(window: tauri::WebviewWindow) -> Result<bool, String> {
    window.is_maximized().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn set_always_on_top(
    window: tauri::WebviewWindow,
    always_on_top: bool,
) -> Result<bool, String> {
    window
        .set_always_on_top(always_on_top)
        .map_err(|e| e.to_string())?;
    Ok(always_on_top)
}

#[tauri::command]
#[specta::specta]
pub fn set_compact_mode(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    compact: bool,
) -> Result<bool, String> {
    use tauri::{LogicalSize, PhysicalSize, Size};

    if compact {
        if let Ok(mut guard) = state.saved_window.lock() {
            if guard.is_none() {
                let maximized = window.is_maximized().unwrap_or(false);
                let size = window.inner_size().unwrap_or(PhysicalSize::new(1380, 860));
                *guard = Some(crate::SavedWindowState {
                    width: size.width,
                    height: size.height,
                    maximized,
                });
            }
        }
        if window.is_maximized().unwrap_or(false) {
            let _ = window.unmaximize();
        }
        let _ = window.set_min_size(Some(Size::Logical(LogicalSize::new(280.0, 400.0))));
        window
            .set_size(Size::Logical(LogicalSize::new(280.0, 400.0)))
            .map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            let hwnd_raw = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
            let _ = tidal_platform_win::apply_compact_dwm_styling(hwnd_raw);
        }
        let _ = window.set_always_on_top(true);
        Ok(true)
    } else {
        #[cfg(windows)]
        {
            let hwnd_raw = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
            let _ = tidal_platform_win::apply_windows11_styling(hwnd_raw, false);
        }
        let _ = window.set_always_on_top(false);
        let _ = window.set_min_size(Some(Size::Logical(LogicalSize::new(1020.0, 680.0))));
        let saved = state.saved_window.lock().ok().and_then(|mut g| g.take());
        if let Some(prev) = saved {
            if prev.width >= 1020 && prev.height >= 680 {
                let _ =
                    window.set_size(Size::Physical(PhysicalSize::new(prev.width, prev.height)));
            } else {
                let _ = window.set_size(Size::Logical(LogicalSize::new(1380.0, 860.0)));
            }
            if prev.maximized {
                let _ = window.maximize();
            }
        } else {
            let _ = window.set_size(Size::Logical(LogicalSize::new(1380.0, 860.0)));
        }
        Ok(false)
    }
}
