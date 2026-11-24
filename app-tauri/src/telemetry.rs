use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::{Channel, InvokeResponseBody};
use tidal_app_engine::{AppEngine, TelemetryFrame};

pub const TELEMETRY_BYTES: usize = 44;

pub fn encode_telemetry_frame(frame: &TelemetryFrame) -> Vec<u8> {
    let mut out = Vec::with_capacity(TELEMETRY_BYTES);
    for level in frame.levels {
        out.extend_from_slice(&level.to_le_bytes());
    }
    out.extend_from_slice(&frame.position_ms.to_le_bytes());
    out.extend_from_slice(&frame.duration_ms.to_le_bytes());
    out.extend_from_slice(&frame.buffered_ms.to_le_bytes());
    debug_assert_eq!(out.len(), TELEMETRY_BYTES);
    out
}

pub const fn should_emit_telemetry(window_visible: bool, playback_active: bool) -> bool {
    window_visible && playback_active
}

pub fn spawn_telemetry_stream(
    engine: Arc<AppEngine>,
    window_visible: Arc<AtomicBool>,
    channel: Channel<InvokeResponseBody>,
    cancel_flag: Arc<AtomicBool>,
) {
    let engine_task = Arc::clone(&engine);
    engine.runtime().spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_micros(16_667));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut was_active = false;
        while !cancel_flag.load(Ordering::Acquire) {
            ticker.tick().await;
            let visible = window_visible.load(Ordering::Relaxed);
            let active = engine_task.is_telemetry_active();

            if should_emit_telemetry(visible, active) {
                was_active = true;
                let frame = engine_task.telemetry_snapshot();
                let bytes = encode_telemetry_frame(&frame);
                if channel.send(InvokeResponseBody::Raw(bytes)).is_err() {
                    break;
                }
            } else if was_active && visible {
                // Emit one final resting frame (zero levels + exact position) so UI settles cleanly.
                was_active = false;
                let mut frame = engine_task.telemetry_snapshot();
                frame.levels = [0.0; 5];
                let bytes = encode_telemetry_frame(&frame);
                if channel.send(InvokeResponseBody::Raw(bytes)).is_err() {
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_44_byte_little_endian_telemetry_frame_exact() {
        let frame = TelemetryFrame {
            levels: [0.1, 0.2, 0.3, 0.4, 0.5],
            position_ms: 12_345,
            duration_ms: 240_000,
            buffered_ms: 8_500,
        };
        let bytes = encode_telemetry_frame(&frame);
        assert_eq!(bytes.len(), 44);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0.1);
        assert_eq!(f32::from_le_bytes(bytes[16..20].try_into().unwrap()), 0.5);
        assert_eq!(u64::from_le_bytes(bytes[20..28].try_into().unwrap()), 12_345);
        assert_eq!(u64::from_le_bytes(bytes[28..36].try_into().unwrap()), 240_000);
        assert_eq!(u64::from_le_bytes(bytes[36..44].try_into().unwrap()), 8_500);
    }

    #[test]
    fn telemetry_gate_emits_zero_frames_when_hidden_or_paused() {
        assert!(!should_emit_telemetry(false, true));
        assert!(!should_emit_telemetry(true, false));
        assert!(!should_emit_telemetry(false, false));
        assert!(should_emit_telemetry(true, true));
    }
}
