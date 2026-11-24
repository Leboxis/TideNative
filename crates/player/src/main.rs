//! CLI de test du moteur audio Bit-Perfect WASAPI (`tidal-player`).
//!
//! Commandes :
//! - `cargo run -p tidal-player -- devices` : liste les DAC / sorties WASAPI disponibles.
//! - `cargo run -p tidal-player -- play <track_id> [HI_RES_LOSSLESS|LOSSLESS|HIGH] [--shared] [--device <id>]`

use std::env;
use std::process::ExitCode;
use tidal_audio_wasapi::{WasapiStreamConfig, enumerate_output_devices};
use tidal_auth::client::TidalClient;
use tidal_auth::credentials::ClientCredentials;
use tidal_core::AudioQuality;
use tidal_player::play_track_blocking;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("devices") => match enumerate_output_devices() {
            Ok(list) => {
                println!("Périphériques de sortie WASAPI actifs ({}) :", list.len());
                for dev in list {
                    let badge = if dev.is_default { " [DÉFAUT]" } else { "" };
                    println!("  • {}{badge}\n    ID: {}", dev.name, dev.id);
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Erreur énumération WASAPI : {e}");
                ExitCode::FAILURE
            }
        },
        Some("play") => {
            let Some(track_str) = args.get(1) else {
                eprintln!("Usage: tidal-player play <track_id> [HI_RES_LOSSLESS|LOSSLESS|HIGH] [--shared] [--device <id>]");
                return ExitCode::FAILURE;
            };
            let Ok(track_id) = track_str.parse::<u64>() else {
                eprintln!("track_id invalide : {track_str}");
                return ExitCode::FAILURE;
            };

            let mut quality = AudioQuality::HiResLossless;
            let mut exclusive = true;
            let mut device_id: Option<String> = None;

            let mut idx = 2;
            while idx < args.len() {
                match args[idx].as_str() {
                    "LOW" => quality = AudioQuality::Low,
                    "HIGH" => quality = AudioQuality::High,
                    "LOSSLESS" => quality = AudioQuality::Lossless,
                    "HI_RES_LOSSLESS" => quality = AudioQuality::HiResLossless,
                    "--shared" => exclusive = false,
                    "--device" => {
                        idx += 1;
                        device_id = args.get(idx).cloned();
                    }
                    _ => {}
                }
                idx += 1;
            }

            let creds = match ClientCredentials::load() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Identifiants TIDAL manquants : {e}");
                    return ExitCode::FAILURE;
                }
            };
            let http = match tidal_auth::http_client() {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("Erreur client HTTP : {e}");
                    return ExitCode::FAILURE;
                }
            };
            let client = match TidalClient::from_store(http, creds) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Aucune session TIDAL active (lancez `cargo run -p tidal-auth -- login`) : {e}");
                    return ExitCode::FAILURE;
                }
            };

            let cfg = WasapiStreamConfig {
                device_id,
                exclusive,
                fallback_to_shared: true,
                target_buffer_ms: 20,
            };

            println!("Ouverture de la piste TIDAL #{track_id} ({quality})...");
            match play_track_blocking(&client, track_id, quality, cfg, |info| {
                println!("▶ {}", info.audiophile_badge());
                println!(
                    "  DAC : {} (conteneur {}-bit / {} bits valides)",
                    info.device_name, info.dac_container_bits, info.dac_valid_bits
                );
            })
            .await
            {
                Ok(()) => {
                    println!("Lecture terminée.");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("Erreur de lecture : {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            println!("tidal-player — Moteur audio Bit-Perfect WASAPI Exclusif");
            println!("Commandes :");
            println!("  devices                                               Lister les DAC / sorties WASAPI");
            println!("  play <trackId> [HI_RES_LOSSLESS] [--shared] [--device <id>]  Lire un titre TIDAL");
            ExitCode::SUCCESS
        }
    }
}
