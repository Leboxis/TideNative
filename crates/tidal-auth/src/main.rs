//! CLI de validation (spike S1 de la feuille de route).
//!
//!   tidal-auth login              # PKCE (Hi-Res) — navigateur + collage de l'URL
//!   tidal-auth device             # Device Code (secours, qualité limitée)
//!   tidal-auth whoami             # recharge le token, refresh si besoin, /v1/sessions
//!   tidal-auth playback <trackId> [QUALITY]   # défaut HI_RES_LOSSLESS
//!   tidal-auth logout

use tidal_auth::manifest::Manifest;
use tidal_auth::pkce::PkceLogin;
use tidal_auth::{ClientCredentials, Result, TidalClient, device, http_client, store, win};
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let http = http_client()?;
    let creds = ClientCredentials::load()?;

    match args.first().map(String::as_str) {
        Some("login") => login_pkce(&http, &creds).await,
        Some("device") => login_device(&http, &creds).await,
        Some("whoami") => whoami(http, creds).await,
        Some("playback") => {
            let id = args.get(1).and_then(|s| s.parse().ok()).ok_or_else(usage)?;
            let quality = args.get(2).map(String::as_str).unwrap_or("HI_RES_LOSSLESS");
            playback(http, creds, id, quality).await
        }
        Some("logout") => {
            store::delete()?;
            println!("Token supprimé du Gestionnaire d'identification.");
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> tidal_auth::AuthError {
    tidal_auth::AuthError::Manifest(
        "usage : tidal-auth <login|device|whoami|playback <trackId> [QUALITY]|logout>".into(),
    )
}

async fn login_pkce(http: &reqwest::Client, creds: &ClientCredentials) -> Result<()> {
    let login = PkceLogin::new(creds)?;
    println!("1. Connectez-vous à TIDAL dans le navigateur qui s'ouvre :\n   {}\n", login.authorize_url);
    if !win::open_in_browser(&login.authorize_url) {
        println!("   (ouvrez l'URL manuellement)");
    }
    println!("2. Après « Autoriser », TIDAL redirige vers une page d'erreur");
    println!("   https://tidal.com/android/login/auth?code=… — c'est normal.");
    println!("3. Copiez l'URL complète de la barre d'adresse et collez-la ici :");

    let mut line = String::new();
    BufReader::new(tokio::io::stdin()).read_line(&mut line).await.ok();
    let token = login.finish(http, creds, &line).await?;
    store::save(&token)?;
    println!("\nConnecté (PKCE). Token enregistré dans le Gestionnaire d'identification ({}).", store::TARGET);
    whoami(http.clone(), creds.clone()).await
}

async fn login_device(http: &reqwest::Client, creds: &ClientCredentials) -> Result<()> {
    let auth = device::start(http, creds).await?;
    let url = auth.browser_url();
    println!("Ouvrez {url} et validez le code {} (expire dans {} s).", auth.user_code, auth.expires_in);
    println!("⚠️  Flux Device Code : qualité potentiellement limitée (AAC 320).");
    win::open_in_browser(&url);
    let token = device::poll(http, creds, &auth).await?;
    store::save(&token)?;
    println!("Connecté (Device Code).");
    whoami(http.clone(), creds.clone()).await
}

async fn whoami(http: reqwest::Client, creds: ClientCredentials) -> Result<()> {
    let client = TidalClient::from_store(http, creds)?;
    let s = client.session().await?;
    let t = client.token_snapshot().await;
    println!(
        "userId={}  countryCode={}  flux={}  expire_dans={} s",
        s.user_id,
        s.country_code,
        if t.is_pkce { "PKCE" } else { "device" },
        t.expires_at.saturating_sub(now())
    );
    Ok(())
}

async fn playback(http: reqwest::Client, creds: ClientCredentials, track_id: u64, quality: &str) -> Result<()> {
    let client = TidalClient::from_store(http, creds)?;
    if client.token_snapshot().await.country_code.is_none() {
        client.session().await?; // mémorise countryCode
    }
    let info = client.playback_info(track_id, quality).await?;
    println!(
        "track={} qualité={} mode={:?} {}-bit / {} Hz  replayGain={:?}",
        info.track_id,
        info.audio_quality,
        info.audio_mode,
        info.bit_depth.unwrap_or(0),
        info.sample_rate.unwrap_or(0),
        info.track_replay_gain
    );
    let manifest = Manifest::decode(&info.manifest_mime_type, &info.manifest)?;
    println!("manifest={}  codecs={:?}  chiffré={}", info.manifest_mime_type, manifest.codecs(), manifest.is_encrypted());
    match &manifest {
        Manifest::Bts(m) => println!("URL directe : {} URL(s)", m.urls.len()),
        Manifest::Dash { mpd_xml } => {
            println!("MPD DASH ({} octets) — aperçu :", mpd_xml.len());
            println!("{}", &mpd_xml[..mpd_xml.len().min(800)]);
        }
    }
    Ok(())
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
