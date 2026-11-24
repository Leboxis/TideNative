use app_tauri::build_specta;
use app_tauri::protocol::parse_cover_path;
use app_tauri::telemetry::{encode_telemetry_frame, should_emit_telemetry};
use specta_typescript::{BigIntExportBehavior, Typescript};
use tidal_app_engine::TelemetryFrame;

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
fn parses_valid_cover_uri_and_rejects_malformed_paths() {
    assert_eq!(
        parse_cover_path("/abc-123/640"),
        Some(("abc-123".to_string(), 640))
    );
    assert_eq!(
        parse_cover_path("/https%3A%2F%2Fresources.tidal.com%2Fimages%2Faa.jpg/320"),
        Some((
            "https://resources.tidal.com/images/aa.jpg".to_string(),
            320
        ))
    );
    assert_eq!(parse_cover_path("/../../etc/passwd"), None);
    assert_eq!(parse_cover_path("/abc/99999"), None);
    assert_eq!(parse_cover_path("//640"), None);
    assert_eq!(
        parse_cover_path("/http%3A%2F%2F127.0.0.1%3A8080%2Fsecret/320"),
        None
    );
    assert_eq!(
        parse_cover_path("/https%3A%2F%2Fevil.example.com%2Fimg.jpg/320"),
        None
    );
}

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
    assert_eq!(
        u64::from_le_bytes(bytes[20..28].try_into().unwrap()),
        12_345
    );
    assert_eq!(
        u64::from_le_bytes(bytes[28..36].try_into().unwrap()),
        240_000
    );
    assert_eq!(u64::from_le_bytes(bytes[36..44].try_into().unwrap()), 8_500);
}

#[test]
fn telemetry_gate_emits_zero_frames_when_hidden_or_paused() {
    assert!(!should_emit_telemetry(false, true));
    assert!(!should_emit_telemetry(true, false));
    assert!(!should_emit_telemetry(false, false));
    assert!(should_emit_telemetry(true, true));
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
