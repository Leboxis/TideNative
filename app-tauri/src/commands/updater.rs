use crate::AppState;
use crate::updater::{self, AppUpdateInfo, UpdateProgressNotice};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};
use tauri_specta::Event;

#[tauri::command]
#[specta::specta]
pub fn get_app_update_status(state: State<'_, AppState>) -> Result<AppUpdateInfo, String> {
    let cached = state
        .updater
        .pending_update
        .lock()
        .ok()
        .and_then(|g| g.clone());
    Ok(updater::get_local_status(cached.as_ref()))
}

#[tauri::command]
#[specta::specta]
pub async fn check_for_updates(state: State<'_, AppState>) -> Result<AppUpdateInfo, String> {
    let (info, plan) = updater::check_remote_updates().await?;

    if let Ok(mut guard) = state.updater.pending_update.lock() {
        if let Some(found) = plan {
            *guard = Some(found);
        } else if !info.update_available {
            *guard = None;
        }
    }

    Ok(info)
}

#[tauri::command]
#[specta::specta]
pub async fn download_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppUpdateInfo, String> {
    let maybe_plan = state
        .updater
        .pending_update
        .lock()
        .ok()
        .and_then(|g| g.clone());

    let plan = match maybe_plan {
        Some(p) => p,
        None => {
            let (info, found) = updater::check_remote_updates().await?;
            if info.pending_restart {
                return Ok(info);
            }
            match found {
                Some(p) => {
                    if let Ok(mut guard) = state.updater.pending_update.lock() {
                        *guard = Some(p.clone());
                    }
                    p
                }
                None => return Ok(info),
            }
        }
    };

    let version = Some(plan.target.Version.clone());
    let _ = UpdateProgressNotice {
        percent: 0,
        stage: "downloading".to_string(),
        version: version.clone(),
    }
    .emit(&app);

    let mut last_percent: i16 = -1;
    let result = updater::download_update_plan(&plan, |percent| {
        if percent != last_percent {
            last_percent = percent;
            let _ = UpdateProgressNotice {
                percent,
                stage: "downloading".to_string(),
                version: version.clone(),
            }
            .emit(&app);
        }
    })
    .await;

    match result {
        Ok(info) => {
            let _ = UpdateProgressNotice {
                percent: 100,
                stage: "ready".to_string(),
                version,
            }
            .emit(&app);
            Ok(info)
        }
        Err(err) => {
            let _ = UpdateProgressNotice {
                percent: 0,
                stage: "error".to_string(),
                version,
            }
            .emit(&app);
            Err(err)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn apply_update_and_restart(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cached = state
        .updater
        .pending_update
        .lock()
        .ok()
        .and_then(|g| g.clone());

    updater::schedule_apply_and_restart(cached.as_ref())?;

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
