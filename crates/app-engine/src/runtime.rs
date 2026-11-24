use crate::{AppEngine, EngineEvent};
use std::sync::Arc;
use std::time::Instant;
use tidal_core::{PlaybackState, PlayerEvent};
use tokio::sync::mpsc;

pub fn spawn_player_event_loop(
    engine: Arc<AppEngine>,
    mut event_rx: mpsc::UnboundedReceiver<PlayerEvent>,
) {
    let engine_task = Arc::clone(&engine);
    engine.runtime().spawn(async move {
        while let Some(event) = event_rx.recv().await {
            handle_player_event(&engine_task, event).await;
        }
    });
}

async fn handle_player_event(engine: &Arc<AppEngine>, event: PlayerEvent) {
    match event {
        PlayerEvent::StateChanged(state) => {
            if let PlaybackState::Error(ref msg) = state {
                engine.emit(EngineEvent::ErrorBanner {
                    message: msg.clone(),
                });
            }
            engine.apply_playback_state(state.clone());
            engine.emit(EngineEvent::PlaybackStateChanged { state });
        }
        PlayerEvent::FormatNegotiated(format) => {
            engine.set_active_format(Some(format.clone()));
            engine.emit(EngineEvent::FormatNegotiated { format });
        }
        PlayerEvent::PositionUpdated {
            position_ms,
            duration_ms,
        } => {
            let now = Instant::now();
            let (updated, current_track) = {
                let mut pb = engine.playback.write().expect("playback lock poisoned");
                let changed = pb.queue.apply_player_position(position_ms, duration_ms, now);
                (changed, pb.queue.current_track.clone())
            };
            if updated {
                engine.update_atomic_position(position_ms, duration_ms);
                if let Some(track) = current_track {
                    if let Ok(mut act) = engine.activity.lock() {
                        act.record_position_tick(track.id, position_ms);
                    }
                    engine.flush_active_listening_progress(false);
                    engine.maybe_scrobble_active_track(&track, position_ms, duration_ms);
                }
            }
        }
        PlayerEvent::BufferHealth {
            buffered_ms,
            underruns: _,
        } => {
            engine.update_atomic_buffered(buffered_ms);
        }
        PlayerEvent::DevicesEnumerated(devices) => {
            engine.set_devices(devices.clone());
            engine.emit(EngineEvent::DevicesEnumerated { devices });
        }
        PlayerEvent::TrackFinished { track_id } => {
            engine.on_track_finished(track_id).await;
        }
        PlayerEvent::TrackTransitioned {
            from_track_id,
            to_track_id,
            duration_ms,
        } => {
            engine
                .on_track_transitioned(from_track_id, to_track_id, duration_ms)
                .await;
        }
    }
}

pub fn is_active_playback(state: &PlaybackState) -> bool {
    matches!(state, PlaybackState::Playing | PlaybackState::Buffering)
}
