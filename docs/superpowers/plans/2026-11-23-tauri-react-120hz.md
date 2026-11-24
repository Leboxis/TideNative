# TideNative — Tauri v2 + React (120 Hz, Windows) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `iced 0.14` presentation layer (`app/`) of TideNative with a sober, premium, 120 Hz Windows desktop frontend built on Tauri v2, Vite, React 19, Tailwind CSS v4, shadcn/ui (Radix UI), `@rare-ui/matrix-orb`, and Motion, while preserving the 100 % native Rust bit-perfect audio, API, cache, and Windows platform core.

**Architecture:** Extract all non-UI orchestration from `app/src/main.rs` into a headless `crates/app-engine` crate consuming the 9 existing domain/audio/platform crates. Mount `Arc<AppEngine>` inside a new `app-tauri/` Tauri v2 binary exposing typed `tauri-specta` commands/events, a custom `tidal-cover://` binary image protocol with Rust-side cover palette extraction, and a 60 Hz binary `Channel<InvokeResponseBody::Raw>` telemetry stream feeding a zero-re-render 120 Hz React UI in `ui/`.

**Tech Stack:** Rust 2024 (`stable-x86_64-pc-windows-gnu`), Tauri v2, `tauri-specta` / `specta`, `zune-jpeg`, Tokio, Vite 6, React 19, TypeScript, pnpm, Tailwind CSS v4, shadcn/ui (Radix UI), `@rare-ui/matrix-orb`, `motion` (`motion/react` >= 14.2), Lucide, Sonner, Zustand, `@tanstack/react-virtual`, Vitest.

**Spec:** [docs/superpowers/specs/2026-11-23-tauri-react-120hz-design.md](file:///C:/Users/KEDI-ADMIN/Documents/Project/tidenative/docs/superpowers/specs/2026-11-23-tauri-react-120hz-design.md)

## Global Constraints

- **Target OS:** Windows 10/11 only; no macOS/Linux abstractions, no Axum server, no web/LAN remote mode.
- **Rust Toolchain:** `stable-x86_64-pc-windows-gnu` (edition 2024, resolver 3) with `target-dir = "C:/Users/KEDI-ADMIN/AppData/Local/tidenative-target"` (`.cargo/config.toml`).
- **Frame Budget:** 120 Hz target (<= 8.33 ms / frame) in WebView2; animate only `transform` and `opacity`; forbid `transition: all`, animated layout properties, animated inherited CSS variables on list parents, and animated `blur() > 20px`.
- **Zero Per-Frame React State:** Scrubber uses `MotionValue` (`scaleX`), timer text updates at 1 Hz (integer seconds), and `MatrixOrb` reads a mutable `Float32Array(5)` inside `requestAnimationFrame`.
- **Visual System:** Spatial Immersion / TIDAL OLED Dark palette from `app/src/theme.rs` (`#040406` canvas, `#0A0B0F` ambient top, `#111116` elevated cards, specular `1px` hairlines, `tabular-nums`, official TIDAL badges `MAX` `#FFD432`, `HIGH` `#33F3FF`, `LOW` `#A8A8B3`, `Bit-Perfect` `#30D158`); do **not** use `design-system/tidenative/tokens.json`.
- **Native Desktop Feel:** `visible: false` + `#040406` background before first React paint (zero white flash), frameless DWM rounded corners + Snap Layouts, `minimize_to_tray`, SMTC attached to `HWND`, disabled default browser `contextmenu` / Ctrl+wheel zoom / image drag / global text selection.
- **Audio & Cold Start Integrity:** Do not regress WASAPI Exclusive bit-perfect playback, gapless transitions, ReplayGain, hot-plug DAC detection, or `< 10 ms` `SnapshotStore` cold boot.
- **Direct Switch:** `app/` (`iced`) is removed from default Cargo workspace members and kept read-only only until Task 12 cleanup.

## Review Focus

- **Stale async responses after rapid skips or typing:** Out-of-order TIDAL radio, lyrics, or search responses arriving after a track change or newer query must be silently discarded via `playback_generation` and `search_seq` without mutating state (pinned in Task 2).
- **Corrupt/missing cover art & bright-white covers:** Invalid `cover_id` or truncated JPEG bytes must return a clean 404/fallback without panicking, and extracted `CoverPalette` luminance must be clamped (`L <= 0.42`) to preserve OLED Dark text contrast (pinned in Task 1 & Task 4).
- **Scrub jitter & idle telemetry drain:** Incoming WASAPI position updates within the 650 ms post-seek `seek_hold_until` window must not snap the scrubber backward, and the 60 Hz binary `Channel` loop must emit 0 frames when paused or hidden to tray (pinned in Task 2, Task 4 & Task 6).
- **Keyboard shortcut collisions with inputs & Radix overlays:** `Space`, `←`, `→`, and `Escape` must never hijack typing in `input`/`textarea`/`[contenteditable]` or fight Radix Dialog/DropdownMenu dismissal stack (pinned in Task 8).
- **Secondary WebView2 environment collision & GNU release link:** Opening the TIDAL login `WebviewWindow` alongside the main window must isolate its `data_directory`, and `cargo build --release` on `stable-x86_64-pc-windows-gnu` must produce a single working `TideNative.exe` (pinned in Task 3, Task 7 & Task 12).

---

## File Structure Map

| Area | Files | Responsibility |
| --- | --- | --- |
| Domain & Palette | `crates/core/src/lib.rs`, `crates/cache/src/palette.rs`, `crates/cache/src/covers.rs` | `CoverPalette` model, `specta::Type` derives, pure-Rust JPEG palette extraction + LRU cache |
| Headless Engine | `crates/app-engine/src/{lib,queue,library,activity,runtime}.rs` | All non-UI orchestration extracted from `app/src/main.rs` (`AppEngine`, `BootstrapState`, `EngineEvent`) |
| Tauri v2 Host | `app-tauri/src/{main,lib,protocol,telemetry,login_window}.rs`, `app-tauri/src/commands/{playback,library,catalog,settings,window}.rs` | `TideNative` binary, DWM/SMTC/Tray, `tidal-cover://`, 60 Hz binary `Channel`, login WebView2, `tauri-specta` |
| Frontend Core | `ui/src/{index.css,bindings.ts}`, `ui/src/lib/{telemetry,covers,shortcuts}.ts`, `ui/src/stores/{playbackStore,libraryStore,uiStore}.ts` | OLED Dark Tailwind v4 tokens, generated IPC bindings, binary telemetry reader, Zustand stores, shortcut router |
| UI Components | `ui/src/components/{layout,player,tracks,modals}/*` | Frameless shell, floating pill nav, ambient backdrop, zero-re-render player bar, `@rare-ui/matrix-orb`, virtualized tracklist, shadcn/Radix modals |
| Views | `ui/src/views/{Discover,Explore,Collection,Playlists,AlbumDetail,ArtistDetail,NowPlaying,QueuePanel,Activity,AudioSettings}View.tsx` | Full feature parity across all 16 legacy `iced` views/panels |
| Packaging & Docs | `Cargo.toml`, `installer/Build-Release-Dist.ps1`, `AGENTS.md` | Workspace switch, single-exe Windows dist build, doc update, `app/` removal |

---

### Task 1: Core Types (`CoverPalette` + `specta`) & Rust Cover Palette Extraction

**Files:**
- Modify: `crates/core/Cargo.toml`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/cache/Cargo.toml`
- Create: `crates/cache/src/palette.rs`
- Modify: `crates/cache/src/covers.rs`
- Modify: `crates/cache/src/lib.rs`

**Interfaces:**
- Consumes: `tidal_cache::CoverCache::get_or_fetch(&self, cover_id: &str, size: u16) -> Result<Arc<Vec<u8>>>`
- Produces:
  - `tidal_core::CoverPalette { pub primary: String, pub secondary: String, pub accent: String, pub surface_tint: String }` (hex `#RRGGBB`, `Serialize + Deserialize + specta::Type`)
  - Feature `specta` on `tidal-core` deriving `specta::Type` on all IPC-exposed structs/enums (`AudioQuality`, `PcmFormat`, `StreamFormatInfo`, `TrackSummary`, `AlbumSummary`, `ArtistSummary`, `ArtistDetailData`, `PlaylistSummary`, `PlaylistFolder`, `MixSummary`, `ExploreHubData`, `SearchResults`, `SearchSuggestions`, `TrackLyrics`, `TrackCredits`, `PlaybackState`, `RepeatMode`, `ReplayGainMode`, `AudioDeviceDescriptor`, `AppPreferences`, `SessionPlaybackState`, `UiSnapshot`, `ListeningActivityStats`, `TrackSortOrder`, `CoverPalette`)
  - `tidal_cache::palette::extract_cover_palette(jpeg_bytes: &[u8]) -> CoverPalette`
  - `tidal_cache::CoverCache::get_palette(&self, cover_id: &str) -> CoverPalette` (async, uses 160px cover bytes + in-memory palette cache, falls back to `CoverPalette::default()` on error)

- [ ] **Step 1: Write failing unit tests for `CoverPalette` extraction and luminance clamping**

In `crates/cache/src/palette.rs` (`#[cfg(test)] mod tests`):
```rust
#[test]
fn fallback_palette_on_invalid_or_empty_bytes() {
    let p = extract_cover_palette(b"not-a-jpeg");
    assert_eq!(p, CoverPalette::default());
    assert_eq!(extract_cover_palette(&[]), CoverPalette::default());
}

#[test]
fn clamps_pure_white_cover_luminance_for_oled_contrast() {
    let white_jpeg = test_solid_jpeg_bytes(255, 255, 255);
    let p = extract_cover_palette(&white_jpeg);
    for hex in [&p.primary, &p.secondary, &p.surface_tint] {
        assert!(relative_luminance_hex(hex) <= 0.42, "too bright: {hex}");
    }
}

#[test]
fn extracts_distinct_saturated_primary_from_colored_jpeg() {
    let cyan_jpeg = test_solid_jpeg_bytes(20, 180, 220);
    let p = extract_cover_palette(&cyan_jpeg);
    assert_ne!(p.primary, CoverPalette::default().primary);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p tidal-cache palette`
Expected: FAIL (module `palette` / `CoverPalette` not defined).

- [ ] **Step 3: Add `CoverPalette` and optional `specta` derives to `crates/core`**

Add optional `specta = { version = "2.0.0-rc.22", features = ["derive"], optional = true }` to `crates/core/Cargo.toml` (with `default = []`). Define `CoverPalette` in `crates/core/src/lib.rs` with `Default` returning OLED-dark safe neutrals (`#111116`, `#0A0B0F`, `#33F3FF`, `#0B0D14`), and gate `#[cfg_attr(feature = "specta", derive(specta::Type))]` across all public IPC models.

- [ ] **Step 4: Implement `extract_cover_palette` and `CoverCache::get_palette` in `crates/cache`**

Add `zune-jpeg = "0.5"` to `crates/cache/Cargo.toml`. Implement `extract_cover_palette(jpeg_bytes: &[u8]) -> CoverPalette` in `crates/cache/src/palette.rs`: decode RGB via `zune_jpeg::JpegDecoder`, stride-sample pixels into an RGB color histogram (quantized 4-bit buckets), pick top vibrant/dominant buckets, convert to HSL to clamp lightness so `primary`/`secondary`/`surface_tint` stay at relative luminance `<= 0.42` while preserving hue, and format `#RRGGBB`. Add an in-memory `HashMap<String, CoverPalette>` cache and `pub async fn get_palette(&self, cover_id: &str) -> CoverPalette` to `CoverCache` in `crates/cache/src/covers.rs` (fetching size `160` and falling back cleanly on any fetch/decode error).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p tidal-core --features specta && cargo test -p tidal-cache`
Expected: PASS (all existing + 3 new palette tests green).

- [ ] **Step 6: Commit**

```bash
git add crates/core crates/cache
git commit -m "feat(core,cache): add CoverPalette, specta derives, and Rust cover palette extraction"
```

---

### Task 2: Headless Application Engine Extraction (`crates/app-engine`)

**Files:**
- Create: `crates/app-engine/Cargo.toml`
- Create: `crates/app-engine/src/lib.rs`
- Create: `crates/app-engine/src/queue.rs`
- Create: `crates/app-engine/src/library.rs`
- Create: `crates/app-engine/src/activity.rs`
- Create: `crates/app-engine/src/runtime.rs`
- Modify: `Cargo.toml` (add `"crates/app-engine"` to workspace members)

**Interfaces:**
- Consumes: `tidal-core`, `tidal-auth`, `tidal-api`, `tidal-cache`, `tidal-audio-wasapi`, `tidal-player` (`spawn_player_actor`, `PlayerHandle`), `tidal-platform-win` (`SmtcCommand`, `SmtcController`, `TrayCommand`, `WebTidalSession`), `CoverCache::get_palette`
- Produces:
  - `AppEngine::boot() -> Arc<Self>` (loads `SnapshotStore` synchronously in `< 10 ms`, enumerates WASAPI devices, opens `CacheDb` + `CoverCache`, restores Last.fm secret & TIDAL token if present, restores `playback_session` stopped)
  - `AppEngine::bootstrap_state(&self) -> BootstrapState`
  - `AppEngine::subscribe_events(&self) -> broadcast::Receiver<EngineEvent>`
  - `AppEngine::attach_smtc(&self, hwnd: isize)` and `AppEngine::dispatch_smtc_command(&self, cmd: SmtcCommand)` / `dispatch_tray_command(&self, cmd: TrayCommand)`
  - `AppEngine::is_telemetry_active(&self) -> bool` and `AppEngine::telemetry_snapshot(&self) -> TelemetryFrame` (`levels: [f32; 5]`, `position_ms: u64`, `duration_ms: u64`, `buffered_ms: u64`)
  - Granular async methods for playback/queue (`play_tracks`, `play_track_by_id`, `toggle_play_pause`, `next_track`, `prev_track`, `seek_ms`, `set_shuffle`, `cycle_repeat`, `queue_play_next`, `queue_add`, `queue_remove`, `queue_move`, `queue_clear`, `play_track_radio`), catalog/search (`refresh_library`, `search_hybrid`, `search_suggestions`, `get_album`, `get_artist`, `get_playlist_tracks`, `get_mix_tracks`, `get_explore_hub`, `get_lyrics`, `get_credits`), mutations (`toggle_favorite_track`, `toggle_favorite_album`, `create_playlist`, `delete_playlist`, `add_track_to_playlist`, `remove_track_from_playlist`), settings/auth (`update_preferences`, `refresh_devices`, `complete_web_login`, `logout`, `connect_lastfm`, `disconnect_lastfm`, `get_activity_stats`, `clear_activity_history`).
  - `EngineEvent` enum (`PlaybackStateChanged`, `TrackChanged { track, palette, queue_index }`, `QueueUpdated`, `FormatNegotiated`, `DevicesEnumerated`, `LibrarySynced`, `SearchResultsReady { seq, ... }`, `ErrorBanner`, `StatusBanner`, `TrayAction(TrayUiAction)`).

- [ ] **Step 1: Write failing unit tests for queue state machine, stale generation/search guards, and `seek_hold_until`**

In `crates/app-engine/src/queue.rs` and `crates/app-engine/src/library.rs`:
```rust
#[test]
fn stale_playback_generation_discards_late_radio_response() {
    let mut q = QueueState::default();
    let gen1 = q.bump_generation();
    let _gen2 = q.bump_generation();
    assert!(!q.accept_radio_tracks(gen1, vec![dummy_track(99)]));
    assert!(q.queue.is_empty());
}

#[test]
fn seek_hold_window_ignores_stale_position_updates_for_650ms() {
    let mut q = QueueState::default();
    let now = Instant::now();
    q.begin_seek(45_000, now);
    assert!(!q.apply_player_position(12_000, 180_000, now + Duration::from_millis(300)));
    assert_eq!(q.position_ms, 45_000);
    assert!(q.apply_player_position(45_700, 180_000, now + Duration::from_millis(700)));
    assert_eq!(q.position_ms, 45_700);
}

#[test]
fn stale_search_seq_is_rejected_and_explicit_ai_filters_apply() {
    let mut lib = LibraryState::default();
    let seq1 = lib.next_search_seq();
    let seq2 = lib.next_search_seq();
    assert!(!lib.is_current_search(seq1));
    assert!(lib.is_current_search(seq2));
    let filtered = filter_and_sort_tracks(&sample_tracks(), false, false, "", TrackSortOrder::TitleAsc);
    assert!(filtered.iter().all(|t| !t.explicit));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tidal-app-engine`
Expected: FAIL (crate does not exist yet).

- [ ] **Step 3: Create `crates/app-engine` and extract orchestration from `app/src/main.rs`**

Port the non-UI state and methods from `app/src/main.rs` and `app/src/state.rs` into focused modules:
- `queue.rs`: `QueueState`, shuffle/unshuffle preserving `original_queue`, repeat (`Off`/`All`/`One`), `bump_generation()`, `begin_seek(..., 650ms)`, `apply_player_position`, gapless next-track resolution, `SessionPlaybackState` snapshot export/restore.
- `library.rs`: `LibraryState`, `filter_and_sort_tracks` (reusing exact explicit/AI filtering rules from `app/src/main.rs`), `search_seq` race guard, FTS5 + TIDAL search coordination, favorites/playlists mutation helpers.
- `activity.rs`: Last.fm credentials load/save via `tidal_auth::load_named_secret("lastfm_session")`, scrobble threshold logic, `CacheDb` listening stats.
- `runtime.rs`: Tokio background loop routing `PlayerEvent`, `SmtcCommand`, and `TrayCommand` into `AppEngine`, updating SMTC metadata/state, persisting `UiSnapshot`, and emitting `EngineEvent` over `tokio::sync::broadcast`.
- `lib.rs`: `AppEngine` using separate `RwLock`s (`playback`, `library`, `preferences`, `api`, `smtc`) so network calls never lock playback/telemetry reads.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tidal-app-engine`
Expected: PASS (all queue, generation, seek-hold, filter, and session-restore tests green).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/app-engine
git commit -m "feat(app-engine): extract headless orchestration crate from iced app"
```

---

### Task 3: Tauri v2 Shell Bootstrap & GNU Toolchain Validation (`app-tauri/`)

**Files:**
- Modify: `Cargo.toml` (add `"app-tauri"` to members, set `default-members` excluding legacy `"app"`)
- Create: `app-tauri/Cargo.toml`
- Create: `app-tauri/build.rs`
- Create: `app-tauri/tauri.conf.json`
- Create: `app-tauri/capabilities/default.json`
- Create: `app-tauri/src/main.rs`
- Create: `app-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `tidal-app-engine::AppEngine`, `tidal-platform-win::{apply_windows11_styling, SmtcController, TrayController, hide_window_to_tray, show_and_focus_window}`
- Produces:
  - Buildable binary `TideNative` on `stable-x86_64-pc-windows-gnu`
  - `app_tauri::run()` launching a frameless (`decorations: false`), `visible: false`, `#040406`-backed main window (`1380×860`, `min: 1020×680`, centered) with DWM styling + SMTC + Tray attached to native `HWND`, `CloseRequested` minimizing to tray when `preferences.minimize_to_tray` is enabled, and `show_main_window` command calling `window.show()` after first React paint.

- [ ] **Step 1: Write failing smoke test for Tauri window config invariants**

In `app-tauri/src/lib.rs` (`#[cfg(test)] mod tests`):
```rust
#[test]
fn tauri_config_enforces_zero_flash_frameless_window() {
    let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let win = &conf["app"]["windows"][0];
    assert_eq!(win["visible"], false);
    assert_eq!(win["decorations"], false);
    assert_eq!(win["backgroundColor"], "#040406");
    assert_eq!(win["width"], 1380);
    assert_eq!(win["height"], 860);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p tidenative-tauri`
Expected: FAIL (crate does not exist).

- [ ] **Step 3: Scaffold `app-tauri/` and configure Tauri v2 + Windows 11 DWM/SMTC/Tray**

Create `app-tauri/Cargo.toml` with `[[bin]] name = "TideNative"`, `tauri = { version = "2", features = ["protocol-asset"] }`, `tauri-build = "2"`, `tauri-specta = { version = "2.0.0-rc.21", features = ["derive", "typescript"] }`, `specta = "2.0.0-rc.22"`, `specta-typescript = "0.0.9"`, `tidal-core = { path = "../crates/core", features = ["specta"] }`, `tidal-app-engine`, `tidal-platform-win`.
Configure `tauri.conf.json` (`frontendDist: "../ui/dist"`, `devUrl: "http://localhost:1420"`, `visible: false`, `decorations: false`, `backgroundColor: "#040406"`, icon referencing `../installer/TideNative.ico`).
In `app-tauri/src/lib.rs`, hook `.setup()` to obtain the native Windows `HWND` via `window.hwnd()`, apply `apply_windows11_styling(hwnd, true)` + Win32 frameless border style (`WS_THICKFRAME` / `WM_NCHITTEST` for Snap Layouts & rounded corners), attach SMTC and Tray, register `on_window_event(WindowEvent::CloseRequested)` for `minimize_to_tray`, and expose `show_main_window` command.

- [ ] **Step 4: Validate compilation and tests on `stable-x86_64-pc-windows-gnu`**

Run: `cargo test -p tidenative-tauri && cargo check -p tidenative-tauri --bin TideNative`
Expected: PASS with 0 errors on `stable-x86_64-pc-windows-gnu` (note: if `tauri-specta` RC has any pin mismatch with `tauri` 2.x, resolve the compatible patch version here before proceeding).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml app-tauri
git commit -m "feat(app-tauri): bootstrap Tauri v2 Windows shell with DWM, SMTC, and Tray"
```

---

### Task 4: High-Frequency IPC Layer (`tidal-cover://`, 60 Hz Binary `Channel`, `tauri-specta` Bindings)

**Files:**
- Create: `app-tauri/src/protocol.rs`
- Create: `app-tauri/src/telemetry.rs`
- Create: `app-tauri/src/commands/mod.rs`
- Create: `app-tauri/src/commands/playback.rs`
- Create: `app-tauri/src/commands/library.rs`
- Create: `app-tauri/src/commands/catalog.rs`
- Create: `app-tauri/src/commands/settings.rs`
- Create: `app-tauri/src/commands/window.rs`
- Modify: `app-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `AppEngine`, `CoverCache::get_or_fetch`, `AppEngine::telemetry_snapshot`, `AppEngine::is_telemetry_active`
- Produces:
  - Async custom URI protocol `tidal-cover://localhost/{cover_id}/{size}` returning `200 image/jpeg` + `Cache-Control: public, max-age=31536000, immutable`, or `404` on invalid/missing cover (no panic)
  - Binary telemetry frame layout (`TELEMETRY_BYTES = 44`, little-endian): bytes `0..20` = `5 × f32` audio levels, `20..28` = `u64` `position_ms`, `28..36` = `u64` `duration_ms`, `36..44` = `u64` `buffered_ms`
  - Command `subscribe_telemetry(channel: Channel<InvokeResponseBody>)` pushing `InvokeResponseBody::Raw(bytes)` at 60 Hz only when `window_visible && engine.is_telemetry_active()`, pausing cleanly at 0 Hz otherwise
  - Complete `tauri-specta` command & event registry exporting `ui/src/bindings.ts` via `export_ts_bindings` test/debug hook (with `BigIntExportBehavior::Number` for safe 53-bit TIDAL IDs/ms).

- [ ] **Step 1: Write failing unit tests for URI parsing, 44-byte telemetry packing, and idle-gate emission**

In `app-tauri/src/protocol.rs` and `app-tauri/src/telemetry.rs`:
```rust
#[test]
fn parses_valid_cover_uri_and_rejects_malformed_paths() {
    assert_eq!(parse_cover_path("/abc-123/640"), Some(("abc-123".to_string(), 640)));
    assert_eq!(parse_cover_path("/../../etc/passwd"), None);
    assert_eq!(parse_cover_path("/abc/99999"), None);
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
    assert_eq!(u64::from_le_bytes(bytes[20..28].try_into().unwrap()), 12_345);
    assert_eq!(u64::from_le_bytes(bytes[36..44].try_into().unwrap()), 8_500);
}

#[test]
fn telemetry_gate_emits_zero_frames_when_hidden_or_paused() {
    assert!(!should_emit_telemetry(false, true));
    assert!(!should_emit_telemetry(true, false));
    assert!(should_emit_telemetry(true, true));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tidenative-tauri protocol telemetry`
Expected: FAIL (modules do not exist).

- [ ] **Step 3: Implement `protocol.rs`, `telemetry.rs`, all command modules, and `tauri-specta` builder**

Implement strict allowlisted cover sizes (`80, 160, 320, 640, 1280`) and percent-decoding for OpenAPI v2 URL cover IDs in `protocol.rs`. Implement `encode_telemetry_frame` and the cancellable 60 Hz `tokio::time::interval(Duration::from_micros(16_667))` task in `telemetry.rs` driven by `AtomicBool` window visibility + `engine.is_telemetry_active()`. Wire all `AppEngine` actions into `commands/{playback,library,catalog,settings,window}.rs` and register them with `tauri_specta::Builder` in `app-tauri/src/lib.rs`, plus a unit test `export_bindings` that writes `../ui/src/bindings.ts`.

- [ ] **Step 4: Run tests and verify `ui/src/bindings.ts` generation**

Run: `cargo test -p tidenative-tauri`
Expected: PASS; `ui/src/bindings.ts` is generated cleanly with all commands and `EngineEvent` types.

- [ ] **Step 5: Commit**

```bash
git add app-tauri ui/src/bindings.ts
git commit -m "feat(app-tauri): add tidal-cover protocol, 60Hz binary telemetry, and tauri-specta IPC"
```

---

### Task 5: Frontend Foundation (`ui/` Vite + React 19 + Tailwind v4 + shadcn/ui + `@rare-ui` + Motion)

**Files:**
- Create: `ui/package.json`
- Create: `ui/vite.config.ts`
- Create: `ui/tsconfig.json`
- Create: `ui/index.html`
- Create: `ui/components.json`
- Create: `ui/src/index.css`
- Create: `ui/src/main.tsx`
- Create: `ui/src/lib/telemetry.ts`
- Create: `ui/src/lib/covers.ts`
- Create: `ui/src/stores/playbackStore.ts`
- Create: `ui/src/stores/libraryStore.ts`
- Create: `ui/src/stores/uiStore.ts`
- Create: `ui/src/components/ui/*` (installed via shadcn CLI: `button`, `dialog`, `dropdown-menu`, `context-menu`, `popover`, `tooltip`, `tabs`, `slider`, `switch`, `select`, `scroll-area`, `command`, `sonner`, `badge`, `separator` + `@rare-ui/matrix-orb`)

**Interfaces:**
- Consumes: `ui/src/bindings.ts` (`commands`, `events`), `tidal-cover://localhost/{id}/{size}`, 44-byte binary telemetry `ArrayBuffer`
- Produces:
  - `decodeTelemetryFrame(buf: ArrayBuffer, outLevels: Float32Array): { positionMs: number; durationMs: number; bufferedMs: number }`
  - `telemetryRuntime` singleton (mutable `levels: Float32Array(5)`, `positionMs`, `durationMs`, `bufferedMs`, `positionListeners` Set for zero-re-render subscribers)
  - `coverUrl(coverId: string | null | undefined, size?: 160 | 320 | 640 | 1280): string | null`
  - Zustand stores `usePlaybackStore`, `useLibraryStore`, `useUiStore` (hydrated from `commands.getBootstrapState()`, subscribed to `EngineEvent`, zero 60/120 Hz fields)

- [ ] **Step 1: Scaffold `ui/` with pnpm, Tailwind v4, shadcn/ui, `@rare-ui/matrix-orb`, Motion >= 14.2, Zustand, TanStack Virtual, Lucide, Sonner, Vitest**

Initialize `ui/package.json` (`dev` on port `1420` strict for Tauri, `build`, `test: vitest run`, `typecheck: tsc --noEmit`). Install dependencies via `pnpm add` / `pnpm add -D`. Configure `ui/index.html` with inline `<style>html,body{background:#040406;color:#F5F5F7;margin:0;overflow:hidden;}</style>`. Configure `ui/src/index.css` with Tailwind CSS v4 `@theme` mapping every color from `app/src/theme.rs` (`--color-canvas: #040406`, `--color-ambient-top: #0A0B0F`, `--color-elevated: #111116`, `--color-tidal-max: #FFD432`, `--color-tidal-high: #33F3FF`, `--color-tidal-low: #A8A8B3`, `--color-bit-perfect: #30D158`, specular borders) and desktop guards (`user-select: none`, `img { -webkit-user-drag: none; }`). Install the pre-made shadcn/ui primitives and `@rare-ui/matrix-orb`.

- [ ] **Step 2: Write failing Vitest tests for binary telemetry decoding and cover URL encoding**

In `ui/src/lib/telemetry.test.ts`:
```ts
it("decodes 44-byte little-endian telemetry frame without allocating new levels array", () => {
  const buf = new ArrayBuffer(44);
  const view = new DataView(buf);
  [0.25, 0.5, 0.75, 0.1, 0.9].forEach((v, i) => view.setFloat32(i * 4, v, true));
  view.setBigUint64(20, 61_500n, true);
  view.setBigUint64(28, 210_000n, true);
  view.setBigUint64(36, 12_000n, true);
  const levels = new Float32Array(5);
  const res = decodeTelemetryFrame(buf, levels);
  expect(levels[0]).toBeCloseTo(0.25);
  expect(levels[4]).toBeCloseTo(0.9);
  expect(res).toEqual({ positionMs: 61_500, durationMs: 210_000, bufferedMs: 12_000 });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `pnpm --dir ui test`
Expected: FAIL (`telemetry.ts` not implemented).

- [ ] **Step 4: Implement `telemetry.ts`, `covers.ts`, and Zustand stores**

Implement `decodeTelemetryFrame` + `telemetryRuntime` (opening Tauri `Channel` once, writing into preallocated `Float32Array(5)`, dispatching to non-React subscribers). Implement `coverUrl` encoding UUIDs or HTTPS OpenAPI v2 cover URLs safely for `tidal-cover://localhost/{encoded}/{size}`. Implement `playbackStore.ts`, `libraryStore.ts`, and `uiStore.ts` + event listener bridge calling `commands.showMainWindow()` on first React paint.

- [ ] **Step 5: Run tests and typecheck**

Run: `pnpm --dir ui test && pnpm --dir ui typecheck && pnpm --dir ui build`
Expected: PASS with 0 TypeScript/Vite errors.

- [ ] **Step 6: Commit**

```bash
git add ui
git commit -m "feat(ui): scaffold Vite, React 19, Tailwind v4 OLED tokens, shadcn/ui, rare-ui, and telemetry runtime"
```

---

### Task 6: 120 Hz PoC Shell — `AppShell`, `TitleBar`, `AmbientBackdrop`, `MatrixOrbCanvas`, `PlayerBar` & Zero-Re-render `Scrubber`

**Files:**
- Create: `ui/src/components/layout/TitleBar.tsx`
- Create: `ui/src/components/layout/TopNavPill.tsx`
- Create: `ui/src/components/layout/AmbientBackdrop.tsx`
- Create: `ui/src/components/layout/AppShell.tsx`
- Create: `ui/src/components/player/QualityBadge.tsx`
- Create: `ui/src/components/player/Scrubber.tsx`
- Create: `ui/src/components/player/MatrixOrbCanvas.tsx`
- Create: `ui/src/components/player/PlayerBar.tsx`
- Create: `ui/src/components/player/Scrubber.test.tsx`

**Interfaces:**
- Consumes: `telemetryRuntime`, `usePlaybackStore`, `useUiStore`, `commands` (playback + window minimize/maximize/close), `CoverPalette`
- Produces:
  - Sober Spatial Immersion chrome: frameless drag-region `TitleBar` + centered floating `TopNavPill` (`radius: 999`, specular `1px` border, WAAPI/CSS active indicator) + floating bottom `PlayerBar` (`radius: 24`)
  - `AmbientBackdrop`: two alternating static pre-rendered gradient layers crossfading via GPU `opacity` (~900 ms) from `CoverPalette` (subtle top glow ~18 % opacity in normal views, deeper spread when `nowPlayingOpen`), zero animated `blur()`
  - `Scrubber`: driven by `useMotionValue(0)` + `transform: scaleX(...)` with local drag override + 650 ms post-seek hold; isolated `TimeDisplay` leaf updating only on `Math.floor(ms / 1000)` change (1 Hz)
  - `MatrixOrbCanvas`: wraps `@rare-ui/matrix-orb` fed by a 120 Hz `requestAnimationFrame` smoother reading `telemetryRuntime.levels` with zero React re-renders.

- [ ] **Step 1: Write failing test pinning zero re-renders during telemetry ticks & scrub hold**

In `ui/src/components/player/Scrubber.test.tsx`:
```tsx
it("updates scrubber scaleX without re-rendering parent and holds user seek value for 650ms", () => {
  const renders = vi.fn();
  render(<ScrubberHarness onParentRender={renders} durationMs={200_000} />);
  expect(renders).toHaveBeenCalledTimes(1);
  telemetryRuntime.pushSynthetic(10_100, 200_000, 5_000);
  telemetryRuntime.pushSynthetic(10_200, 200_000, 5_000);
  expect(renders).toHaveBeenCalledTimes(1);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --dir ui test Scrubber`
Expected: FAIL (`Scrubber` not implemented).

- [ ] **Step 3: Implement Shell, `AmbientBackdrop`, `MatrixOrbCanvas`, `QualityBadge`, `Scrubber`, and `PlayerBar`**

Build components cleanly using shadcn/ui (`Button`, `Tooltip`, `Slider` for volume), Lucide icons, and Motion (`motion/react` >= 14.2) with explicit `transform`/`opacity` animations only. Ensure `QualityBadge` displays exact official TIDAL colors (`MAX` `#FFD432`, `HIGH` `#33F3FF`, `LOW` `#A8A8B3`) and `StreamFormatInfo` bit-perfect pill (`#30D158`, `tabular-nums`). Disable global browser `contextmenu` and Ctrl+wheel zoom in `AppShell`.

- [ ] **Step 4: Run tests, build frontend, and verify Tauri PoC boots at 120 Hz with zero white flash**

Run: `pnpm --dir ui test && pnpm --dir ui build && cargo check -p tidenative-tauri`
Expected: PASS. Manual PoC check on `cargo run -p tidenative-tauri`: window appears directly in `#040406` (zero white flash), frameless drag + Snap Layouts work, `MatrixOrb` and `AmbientBackdrop` hold < 8.33 ms/frame in WebView2 DevTools Performance trace.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components
git commit -m "feat(ui): implement 120Hz Spatial Immersion shell, ambient backdrop, MatrixOrb, and zero-rerender PlayerBar"
```

---

### Task 7: Integrated WebView2 TIDAL Login & Core Listening Views (`Discover`, `VirtualTrackList`, `Favorites`)

**Files:**
- Create: `app-tauri/src/login_window.rs`
- Modify: `app-tauri/src/commands/settings.rs` (add `start_tidal_web_login`)
- Create: `ui/src/components/modals/LoginDialog.tsx`
- Create: `ui/src/components/tracks/VirtualTrackList.tsx`
- Create: `ui/src/views/DiscoverView.tsx`
- Create: `ui/src/views/CollectionView.tsx`

**Interfaces:**
- Consumes: `AppEngine::complete_web_login`, `useLibraryStore`, `coverUrl`, `@tanstack/react-virtual`
- Produces:
  - `login_window::open_tidal_login_window(app: &AppHandle)` opening a secondary Tauri `WebviewWindow` on `https://listen.tidal.com` with isolated `data_directory` (`{app_data_dir}/webview-login-profile`), hooking `ICoreWebView2` via `window.with_webview` to intercept OAuth token / `Authorization: Bearer` identically to legacy `crates/platform-win/src/webview_login.rs`, then closing itself immediately upon capture
  - `VirtualTrackList`: fixed-height virtualized track rows (`@tanstack/react-virtual`, overscan 8, `contain: layout style paint`), cover thumbnail (`loading="lazy" decoding="async"`), `QualityBadge`, explicit tag, favorite heart toggle, double-click play, context menu trigger
  - `DiscoverView` (hero banner, personalized mixes `userDailyMixes`/`userDiscoveryMixes`/`userNewReleaseMixes`, recent tracks grid) & `CollectionView` (Tabs: Favorites tracks with sort/filter + saved Albums grid + followed Artists grid + Mixes).

- [ ] **Step 1: Write failing test verifying isolated login WebView2 profile path**

In `app-tauri/src/login_window.rs`:
```rust
#[test]
fn login_webview_uses_dedicated_isolated_data_directory() {
    let base = PathBuf::from("C:/AppData/TideNative");
    let login_dir = login_profile_dir(&base);
    assert_ne!(login_dir, base);
    assert!(login_dir.ends_with("webview-login-profile"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p tidenative-tauri login_window`
Expected: FAIL (`login_window` not defined).

- [ ] **Step 3: Implement `login_window.rs`, `LoginDialog`, `VirtualTrackList`, `DiscoverView`, and `CollectionView`**

Port the `ICoreWebView2` `WebResourceRequested` / `WebResourceResponseReceived` interception from `crates/platform-win/src/webview_login.rs` onto Tauri's `WebviewWindow::with_webview`. Build `VirtualTrackList` and wire `DiscoverView` + `CollectionView` with real TIDAL artwork and shadcn `Tabs`/`Select`/`Input` filter controls.

- [ ] **Step 4: Run Rust + UI tests and verify build**

Run: `cargo test -p tidenative-tauri && pnpm --dir ui test && pnpm --dir ui typecheck`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app-tauri ui/src
git commit -m "feat(auth,views): add isolated WebView2 TIDAL login, virtualized tracklist, Discover, and Collection views"
```

---

### Task 8: Explore, Hybrid Instant Search (`Command` `Ctrl+K`), Keyboard Shortcuts & Context Menus

**Files:**
- Create: `ui/src/lib/shortcuts.ts`
- Create: `ui/src/lib/shortcuts.test.ts`
- Create: `ui/src/components/modals/CommandSearch.tsx`
- Create: `ui/src/components/tracks/TrackContextMenu.tsx`
- Create: `ui/src/views/ExploreView.tsx`

**Interfaces:**
- Consumes: `commands.searchHybrid`, `commands.searchSuggestions`, `commands.getExploreHub`, `commands.playTrackRadio`, `commands.queuePlayNext`, `commands.queueAdd`, `usePlaybackStore`, `useUiStore`
- Produces:
  - `shouldIgnoreGlobalShortcut(target: EventTarget | null, hasOpenRadixLayer: boolean): boolean`
  - `registerGlobalShortcuts()` handling `Ctrl+K` (open `CommandSearch` / focus search), `Alt+←` / `Alt+→` (nav history), `Ctrl+←` / `Ctrl+→` (prev/next track), `Space` (play/pause), `←` / `→` (±10 s seek), and `Escape` layered dismissal synced with Radix UI
  - `CommandSearch` (shadcn `CommandDialog` instant local SQLite FTS5 + debounced TIDAL suggestions)
  - `ExploreView` (editorial hub categories, instant catalog search results grouped by Top Hits / Tracks / Albums / Artists / Playlists)
  - `TrackContextMenu` (Radix `ContextMenu` & `DropdownMenu` with Motion `forceMount` + `asChild` + `AnimatePresence`: Play next, Add to queue, Start Track Radio, Toggle favorite, Add/Remove playlist, View Album/Artist, Credits, Copy TIDAL link).

- [ ] **Step 1: Write failing Vitest tests for keyboard shortcut isolation**

In `ui/src/lib/shortcuts.test.ts`:
```ts
it("ignores Space and Arrow keys when an input or Radix dialog is active", () => {
  const input = document.createElement("input");
  expect(shouldIgnoreGlobalShortcut(input, false)).toBe(true);
  expect(shouldIgnoreGlobalShortcut(document.body, true)).toBe(true);
  expect(shouldIgnoreGlobalShortcut(document.body, false)).toBe(false);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --dir ui test shortcuts`
Expected: FAIL (`shortcuts.ts` not implemented).

- [ ] **Step 3: Implement `shortcuts.ts`, `CommandSearch`, `TrackContextMenu`, and `ExploreView`**

Implement shortcut filtering checking `target.closest("input, textarea, [contenteditable='true']")` and open Radix portal layers (`[data-radix-popper-content-wrapper], [role="dialog"]`). Implement `CommandSearch`, `TrackContextMenu`, and `ExploreView` consuming `search_seq`-guarded backend results.

- [ ] **Step 4: Run tests and typecheck**

Run: `pnpm --dir ui test && pnpm --dir ui typecheck`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src
git commit -m "feat(ui): add Explore view, Ctrl+K hybrid command search, context menus, and guarded keyboard shortcuts"
```

---

### Task 9: Catalog Detail Views & Playlist Management (`Album`, `Artist`, `Playlists`, CRUD Modals)

**Files:**
- Create: `ui/src/views/AlbumDetailView.tsx`
- Create: `ui/src/views/ArtistDetailView.tsx`
- Create: `ui/src/views/PlaylistsView.tsx`
- Create: `ui/src/components/modals/PlaylistModals.tsx`
- Create: `ui/src/components/modals/CreditsDialog.tsx`

**Interfaces:**
- Consumes: `commands.getAlbum`, `commands.getArtist`, `commands.getPlaylistTracks`, `commands.getMixTracks`, `commands.createPlaylist`, `commands.deletePlaylist`, `commands.addTrackToPlaylist`, `commands.removeTrackFromPlaylist`, `commands.toggleFavoriteAlbum`, `commands.getCredits`, `VirtualTrackList`, `TrackContextMenu`
- Produces:
  - `AlbumDetailView` (header with high-res cover `640`, Play/Shuffle/Add album to queue/Favorite album actions, `VirtualTrackList`, similar albums shelf)
  - `ArtistDetailView` (artist profile art, Top Tracks, Discography tabs: Albums / EPs & Singles / Compilations, Artist Radio, Similar Artists)
  - `PlaylistsView` (All / Folders / Playlists filter pills, Folder drill-down, Playlist detail with track sort/filter and remove-from-playlist context)
  - `PlaylistModals` (Create Playlist dialog + Delete Confirmation dialog via shadcn `Dialog` + Motion `forceMount`) and `CreditsDialog`.

- [ ] **Step 1: Write failing Vitest test for artist discography tab filtering & playlist folder filter**

In `ui/src/views/PlaylistsView.test.tsx`:
```tsx
it("filters between All, Folders, and Playlists cleanly", () => {
  render(<PlaylistsViewHarness />);
  fireEvent.click(screen.getByRole("tab", { name: /dossiers/i }));
  expect(screen.getByText("Hi-Res Jazz")).toBeInTheDocument();
  expect(screen.queryByText("Night Drive")).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --dir ui test PlaylistsView`
Expected: FAIL (`PlaylistsView` not implemented).

- [ ] **Step 3: Implement `AlbumDetailView`, `ArtistDetailView`, `PlaylistsView`, `PlaylistModals`, and `CreditsDialog`**

Compose views using shadcn `Tabs`, `Dialog`, `Button`, `ScrollArea`, and `VirtualTrackList`, with `Sonner` toast feedback on playlist create/delete/add/remove mutations.

- [ ] **Step 4: Run tests and build**

Run: `pnpm --dir ui test && pnpm --dir ui typecheck && pnpm --dir ui build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src
git commit -m "feat(views): implement AlbumDetail, ArtistDetail, Playlists with folders, and CRUD/Credits modals"
```

---

### Task 10: Immersive `NowPlayingView` & Drag-and-Drop `QueuePanel`

**Files:**
- Create: `ui/src/views/NowPlayingView.tsx`
- Create: `ui/src/views/QueuePanel.tsx`
- Create: `ui/src/views/NowPlayingView.test.tsx`

**Interfaces:**
- Consumes: `usePlaybackStore`, `telemetryRuntime`, `commands.getLyrics`, `commands.getCredits`, `commands.queueMove`, `commands.queueRemove`, `commands.queueClear`, `commands.playQueueAt`, `AmbientBackdrop`, `MatrixOrbCanvas`
- Produces:
  - `NowPlayingView`: full-window overlay animating in/out strictly via GPU `transform: translateY` + `opacity` (Motion `AnimatePresence`), deeper `CoverPalette` ambient spread, large crisp cover (`1280`), live `MatrixOrbCanvas`, and 3 tabs (`Aperçu`, `Paroles` with synced active-line auto-scroll driven by 1 Hz time listener, `Crédits`)
  - `QueuePanel`: slide-over drawer animating strictly via `transform: translateX` (never resizing main content flex width), tabs `À suivre` / `Historique`, reorderable upcoming tracks via Motion `Reorder` / drag handle calling `commands.queueMove(from, to)`, remove & clear actions.

- [ ] **Step 1: Write failing test for active synced lyrics line index calculation**

In `ui/src/views/NowPlayingView.test.tsx`:
```ts
it("resolves the active synced lyric line index for current playback ms", () => {
  const lines = [
    { startMs: 0, text: "Intro" },
    { startMs: 12_000, text: "Verse 1" },
    { startMs: 25_000, text: "Chorus" },
  ];
  expect(findActiveLyricIndex(lines, 11_999)).toBe(0);
  expect(findActiveLyricIndex(lines, 12_000)).toBe(1);
  expect(findActiveLyricIndex(lines, 30_000)).toBe(2);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --dir ui test NowPlayingView`
Expected: FAIL (`findActiveLyricIndex` not defined).

- [ ] **Step 3: Implement `NowPlayingView` and `QueuePanel`**

Use binary search for `findActiveLyricIndex` (triggered only on lyric boundary crossings, never per frame). Ensure `QueuePanel` overlays on its own compositor layer (`position: fixed`, `translateX`) so toggling it never causes main-view layout reflow.

- [ ] **Step 4: Run tests and typecheck**

Run: `pnpm --dir ui test && pnpm --dir ui typecheck`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/views
git commit -m "feat(views): implement 120Hz NowPlaying overlay with synced lyrics and drag-reorder QueuePanel"
```

---

### Task 11: Listening `ActivityView`, Last.fm Scrobbling & Audiophile `AudioSettingsView`

**Files:**
- Create: `ui/src/views/ActivityView.tsx`
- Create: `ui/src/views/AudioSettingsView.tsx`

**Interfaces:**
- Consumes: `commands.getActivityStats`, `commands.clearActivityHistory`, `commands.connectLastfm`, `commands.disconnectLastfm`, `commands.updatePreferences`, `commands.refreshDevices`, `commands.openDataFolder`, `usePlaybackStore`, `useUiStore`
- Produces:
  - `ActivityView`: period selector (`30 jours` / `Depuis toujours`), listening totals (`tabular-nums`), top artists/tracks cards, clear history confirmation, Last.fm scrobbling status & connection form (credentials stored in Windows Credential Manager via `tidal-auth`)
  - `AudioSettingsView`: WASAPI output device selector (live-updated on USB DAC hot-plug), quality tier selector (`MAX` / `HIGH` / `LOW` with TIDAL descriptions), toggles for Exclusive mode, Fallback shared, Bit-Perfect lock, Normalize volume + ReplayGain mode (`Off`/`Track`/`Album`), Gapless, Autoplay radio, Minimize to tray, Explicit & AI content filters, live `StreamFormatInfo` diagnostic readout, and Open Data Folder / Logout actions.

- [ ] **Step 1: Write failing Vitest test verifying Bit-Perfect Lock disables software volume and ReplayGain updates preferences**

In `ui/src/views/AudioSettingsView.test.tsx`:
```tsx
it("sends updated AppPreferences when toggling WASAPI Exclusive and ReplayGain mode", async () => {
  const spy = vi.spyOn(commands, "updatePreferences").mockResolvedValue({ status: "ok", data: null });
  render(<AudioSettingsView />);
  fireEvent.click(screen.getByRole("switch", { name: /mode exclusif wasapi/i }));
  await waitFor(() => expect(spy).toHaveBeenCalledWith(expect.objectContaining({ exclusive_mode: false })));
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --dir ui test AudioSettingsView`
Expected: FAIL (`AudioSettingsView` not implemented).

- [ ] **Step 3: Implement `ActivityView` and `AudioSettingsView`**

Use shadcn `Switch`, `Select`, `Tabs`, `Button`, `Separator`, and `Dialog` (for clear-history / Last.fm login). Keep every audio preference wired 1-to-1 with `AppPreferences` in `crates/core/src/lib.rs`.

- [ ] **Step 4: Run full frontend & backend test suites**

Run: `cargo test --workspace && pnpm --dir ui test && pnpm --dir ui build`
Expected: PASS (0 failures across all Rust crates and React tests).

- [ ] **Step 5: Commit**

```bash
git add ui/src/views
git commit -m "feat(views): implement ActivityView with Last.fm and audiophile AudioSettingsView"
```

---

### Task 12: 120 Hz Audit, Windows Packaging (`Build-Release-Dist.ps1`), Docs Update & Legacy `app/` Removal

**Files:**
- Modify: `installer/Build-Release-Dist.ps1`
- Modify: `Cargo.toml` (remove `"app"` completely from workspace)
- Delete: `app/` (legacy `iced 0.14` crate)
- Modify: `AGENTS.md` (document Tauri v2 + React + `crates/app-engine` architecture)

**Interfaces:**
- Consumes: `ui/dist`, `app-tauri` release binary (`TideNative.exe`)
- Produces:
  - Verified release executable `C:\Users\KEDI-ADMIN\AppData\Local\tidenative-target\release\TideNative.exe` + `dist/TideNative-0.1.0-Windows-x64.zip`
  - Clean workspace with zero `iced` dependencies remaining in `Cargo.lock`.

- [ ] **Step 1: Audit 120 Hz performance in WebView2 DevTools**

Run the app (`cargo run --release -p tidenative-tauri`) and record Performance traces across:
1. Scrolling a 1,000+ track Favorites list (`VirtualTrackList`),
2. Scrubbing + playing audio with `MatrixOrb` and `AmbientBackdrop` crossfade,
3. Opening/closing `NowPlayingView` and `QueuePanel`,
4. Minimizing to tray (verify 0 IPC telemetry frames and near-0 % CPU).
Fix any animation exceeding 8.33 ms / frame (layout thrash, uncomposited paint, or stray re-render) before continuing.

- [ ] **Step 2: Update `installer/Build-Release-Dist.ps1`, remove legacy `app/`, and update `Cargo.toml` + `AGENTS.md`**

Update `installer/Build-Release-Dist.ps1` to run `pnpm --dir ui build` before `cargo build --release -p tidenative-tauri --bin TideNative`. Delete `app/`, remove `"app"` from `Cargo.toml` members, run `cargo check --workspace` to prune `iced` from `Cargo.lock`, and update `AGENTS.md` with the new Tauri v2 / React architecture and IPC conventions.

- [ ] **Step 3: Execute full end-to-end release build and test verification**

Run:
```bash
cargo test --workspace
pnpm --dir ui test && pnpm --dir ui typecheck && pnpm --dir ui build
pwsh -NoProfile -File installer/Build-Release-Dist.ps1
```
Expected: All Rust and Vitest tests PASS, `TideNative.exe` compiles cleanly on `stable-x86_64-pc-windows-gnu`, and `dist/TideNative-0.1.0-Windows-x64.zip` is assembled without errors.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: finalize Tauri v2 Windows packaging, update AGENTS.md, and remove legacy iced app"
```
