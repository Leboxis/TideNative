pub mod commands;
pub mod login_window;
pub mod protocol;
pub mod telemetry;

#[cfg(debug_assertions)]
use specta_typescript::{BigIntExportBehavior, Typescript};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_specta::{Builder as SpectaBuilder, Event, collect_commands, collect_events};
use tidal_app_engine::{AppEngine, EngineEvent, TrayUiAction};
use tidal_platform_win::{
    TrayCommand, TrayController, apply_windows11_styling, hide_window_to_tray,
    show_and_focus_window,
};
use tokio::sync::mpsc;

#[derive(Debug, Clone, serde::Serialize, specta::Type, tauri_specta::Event)]
pub struct EngineEventNotice(pub EngineEvent);

#[derive(Debug, Clone, Copy)]
pub struct SavedWindowState {
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

pub struct AppState {
    pub engine: Arc<AppEngine>,
    pub window_visible: Arc<AtomicBool>,
    pub main_hwnd: AtomicIsize,
    pub has_tray: AtomicBool,
    pub telemetry_cancel: Mutex<Option<Arc<AtomicBool>>>,
    pub saved_window: Mutex<Option<SavedWindowState>>,
}

pub fn build_specta() -> SpectaBuilder<tauri::Wry> {
    SpectaBuilder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::playback::get_bootstrap_state,
            commands::playback::play_tracks,
            commands::playback::play_queue_index,
            commands::playback::play_track_by_id,
            commands::playback::toggle_play_pause,
            commands::playback::next_track,
            commands::playback::prev_track,
            commands::playback::seek_ms,
            commands::playback::set_volume,
            commands::playback::toggle_mute,
            commands::playback::set_shuffle,
            commands::playback::cycle_repeat,
            commands::playback::queue_play_next,
            commands::playback::queue_add,
            commands::playback::queue_remove,
            commands::playback::queue_move,
            commands::playback::queue_clear,
            commands::playback::play_track_radio,
            commands::playback::play_artist_radio,
            commands::library::refresh_library,
            commands::library::toggle_favorite_track,
            commands::library::toggle_favorite_album,
            commands::library::toggle_favorite_artist,
            commands::library::get_playlist_tracks,
            commands::library::create_playlist,
            commands::library::delete_playlist,
            commands::library::add_track_to_playlist,
            commands::library::remove_track_from_playlist,
            commands::catalog::search_hybrid,
            commands::catalog::search_suggestions,
            commands::catalog::get_explore_hub,
            commands::catalog::get_explore_category,
            commands::catalog::get_album,
            commands::catalog::get_artist,
            commands::catalog::get_mix_tracks,
            commands::catalog::get_lyrics,
            commands::catalog::get_credits,
            commands::settings::update_preferences,
            commands::settings::refresh_devices,
            commands::settings::open_tidal_login,
            commands::settings::logout,
            commands::settings::connect_lastfm,
            commands::settings::disconnect_lastfm,
            commands::settings::get_activity_stats,
            commands::settings::clear_activity_history,
            commands::settings::open_data_folder,
            commands::window::show_main_window,
            commands::window::minimize_window,
            commands::window::toggle_maximize_window,
            commands::window::close_main_window,
            commands::window::is_window_maximized,
            commands::window::set_compact_mode,
        ])
        .events(collect_events![EngineEventNotice])
}

fn build_tray_icon_rgba() -> (Vec<u8>, u32, u32) {
    let size: u32 = 32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let diamonds: [(f32, f32, f32); 4] = [
        (8.0, 12.0, 4.5),
        (16.0, 12.0, 4.5),
        (24.0, 12.0, 4.5),
        (16.0, 20.0, 4.5),
    ];
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            let idx = ((y * size + x) * 4) as usize;
            let in_diamond = diamonds
                .iter()
                .any(|&(cx, cy, r)| (fx - cx).abs() + (fy - cy).abs() <= r);
            if in_diamond {
                rgba[idx] = 0x33;
                rgba[idx + 1] = 0xF3;
                rgba[idx + 2] = 0xFF;
                rgba[idx + 3] = 0xFF;
            } else {
                let dx = (fx - 16.0).abs();
                let dy = (fy - 16.0).abs();
                if dx <= 15.0 && dy <= 15.0 && (dx + dy <= 25.0) {
                    rgba[idx] = 0x0A;
                    rgba[idx + 1] = 0x0B;
                    rgba[idx + 2] = 0x0F;
                    rgba[idx + 3] = 0xFF;
                }
            }
        }
    }
    (rgba, size, size)
}

fn spawn_tray_and_event_bridge(
    app_handle: AppHandle,
    engine: Arc<AppEngine>,
    window_visible: Arc<AtomicBool>,
    hwnd_raw: isize,
) -> bool {
    let (tray_tx, mut tray_rx) = mpsc::unbounded_channel::<TrayCommand>();
    let (rgba, w, h) = build_tray_icon_rgba();
    let tray_controller = TrayController::spawn(rgba, w, h, tray_tx).ok();
    let has_tray = tray_controller.is_some();

    let engine_tray = Arc::clone(&engine);
    engine.runtime().spawn(async move {
        let _keep_tray_alive = tray_controller;
        while let Some(cmd) = tray_rx.recv().await {
            engine_tray.dispatch_tray_command(cmd).await;
        }
    });

    let mut events_rx = engine.subscribe_events();
    let app_for_events = app_handle.clone();
    let visible_flag = Arc::clone(&window_visible);
    engine.runtime().spawn(async move {
        while let Ok(event) = events_rx.recv().await {
            if let EngineEvent::TrayAction { action } = &event {
                match action {
                    TrayUiAction::ShowWindow => {
                        visible_flag.store(true, Ordering::Release);
                        if hwnd_raw != 0 {
                            show_and_focus_window(hwnd_raw as u64);
                        }
                        if let Some(win) = app_for_events.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                        }
                    }
                    TrayUiAction::QuitApp => {
                        app_for_events.exit(0);
                    }
                }
            }
            let _ = EngineEventNotice(event).emit(&app_for_events);
        }
    });

    has_tray
}

pub fn run() {
    let specta_builder = build_specta();
    #[cfg(debug_assertions)]
    {
        let _ = specta_builder.export(
            Typescript::default().bigint(BigIntExportBehavior::Number),
            "../ui/src/bindings.ts",
        );
    }

    let engine = AppEngine::boot();
    let window_visible = Arc::new(AtomicBool::new(false));
    let app_state = AppState {
        engine: Arc::clone(&engine),
        window_visible: Arc::clone(&window_visible),
        main_hwnd: AtomicIsize::new(0),
        has_tray: AtomicBool::new(false),
        telemetry_cancel: Mutex::new(None),
        saved_window: Mutex::new(None),
    };

    fn raw_telemetry_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
        tauri::generate_handler![commands::playback::subscribe_telemetry]
    }

    let specta_handler = specta_builder.invoke_handler();
    let raw_handler = raw_telemetry_handler();
    let cover_engine = Arc::clone(&engine);

    tauri::Builder::default()
        .manage(app_state)
        .register_asynchronous_uri_scheme_protocol("tidal-cover", move |_ctx, req, responder| {
            let cache = cover_engine.cover_cache();
            let path = req.uri().path().to_string();
            cover_engine.runtime().spawn(async move {
                let response = protocol::resolve_cover_response(cache, &path).await;
                responder.respond(response);
            });
        })
        .invoke_handler(move |invoke| {
            if invoke.message.command() == "subscribe_telemetry" {
                raw_handler(invoke)
            } else {
                specta_handler(invoke)
            }
        })
        .setup(move |app| {
            specta_builder.mount_events(app);
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(windows)]
                let hwnd_raw = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
                #[cfg(not(windows))]
                let hwnd_raw: isize = 0;

                let state = app.state::<AppState>();
                state.main_hwnd.store(hwnd_raw, Ordering::Release);

                if hwnd_raw != 0 {
                    let _ = apply_windows11_styling(hwnd_raw, false);
                    engine.attach_smtc(hwnd_raw);
                }

                let has_tray = spawn_tray_and_event_bridge(
                    app.handle().clone(),
                    Arc::clone(&engine),
                    Arc::clone(&window_visible),
                    hwnd_raw,
                );
                state.has_tray.store(has_tray, Ordering::Release);

                // Failsafe: ensure main window reveals even if WebView2 delays JS timers.
                let fallback_app = app.handle().clone();
                let fallback_visible = Arc::clone(&window_visible);
                engine.runtime().spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
                    if let Some(win) = fallback_app.get_webview_window("main") {
                        if !win.is_visible().unwrap_or(false) {
                            fallback_visible.store(true, Ordering::Release);
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            let state = window.state::<AppState>();
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    state.engine.persist_snapshot();
                    let hwnd = state.main_hwnd.load(Ordering::Acquire);
                    let has_tray = state.has_tray.load(Ordering::Acquire);
                    if state.engine.preferences().minimize_to_tray && hwnd != 0 && has_tray {
                        api.prevent_close();
                        state.window_visible.store(false, Ordering::Release);
                        hide_window_to_tray(hwnd as u64);
                    }
                }
                WindowEvent::Focused(focused) => {
                    if *focused {
                        state.window_visible.store(true, Ordering::Release);
                    }
                }
                WindowEvent::Destroyed => {
                    state.window_visible.store(false, Ordering::Release);
                    if let Ok(mut guard) = state.telemetry_cancel.lock() {
                        if let Some(flag) = guard.take() {
                            flag.store(true, Ordering::Release);
                        }
                    }
                    state.engine.persist_snapshot();
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running TideNative Tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tauri_config_enforces_zero_flash_frameless_window() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let win = &conf["app"]["windows"][0];
        assert_eq!(win["visible"], false);
        assert_eq!(win["decorations"], false);
        assert_eq!(win["backgroundColor"], "#040406");
        assert_eq!(win["width"], 1380);
        assert_eq!(win["height"], 860);
    }

    #[test]
    fn export_typescript_bindings() {
        let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src");
        std::fs::create_dir_all(&out_dir).expect("create ui/src");
        let target = out_dir.join("bindings.ts");
        build_specta()
            .export(
                Typescript::default().bigint(BigIntExportBehavior::Number),
                &target,
            )
            .expect("export tauri-specta TypeScript bindings");
        let content = std::fs::read_to_string(&target).expect("read generated bindings.ts");
        assert!(content.contains("getBootstrapState"));
        assert!(content.contains("EngineEventNotice"));
        assert!(content.contains("CoverPalette"));
    }
}
