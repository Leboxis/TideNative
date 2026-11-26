use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, exit};
use std::sync::Mutex;

pub const DEFAULT_UPDATE_SOURCE_URL: &str = "https://github.com/Leboxis/TideNative";

const HOOK_CLI_FLAGS: &[&str] = &[
    "--veloapp-install",
    "--veloapp-updated",
    "--veloapp-obsolete",
    "--veloapp-uninstall",
];
const HOOK_ENV_RESTART: &str = "VELOPACK_RESTART";
const DEFAULT_CHANNEL: &str = "win";

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AppUpdateInfo {
    pub current_version: String,
    pub is_installed: bool,
    pub update_available: bool,
    pub pending_restart: bool,
    pub available_version: Option<String>,
    pub release_notes: Option<String>,
    pub is_delta: bool,
    pub download_size_bytes: Option<u64>,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct UpdateProgressNotice {
    pub percent: i16,
    pub stage: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case)]
pub struct VelopackAssetFeed {
    #[serde(alias = "assets", default)]
    pub Assets: Vec<VelopackAsset>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case)]
pub struct VelopackAsset {
    #[serde(alias = "packageId", default)]
    pub PackageId: String,
    #[serde(alias = "version", default)]
    pub Version: String,
    #[serde(alias = "type", default)]
    pub Type: String,
    #[serde(alias = "fileName", default)]
    pub FileName: String,
    #[serde(alias = "sha1", default)]
    pub SHA1: String,
    #[serde(alias = "sha256", default)]
    pub SHA256: String,
    #[serde(alias = "size", default)]
    pub Size: u64,
    #[serde(alias = "notesMarkdown", default)]
    pub NotesMarkdown: String,
    #[serde(skip)]
    pub download_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingUpdatePlan {
    pub target: VelopackAsset,
    pub base_package_path: Option<PathBuf>,
    pub deltas: Vec<VelopackAsset>,
    pub source_url: String,
}

#[derive(Default)]
pub struct UpdaterState {
    pub pending_update: Mutex<Option<PendingUpdatePlan>>,
}

#[derive(Debug, Clone)]
pub struct VelopackInstall {
    pub root_dir: PathBuf,
    pub packages_dir: PathBuf,
    pub update_exe: PathBuf,
    pub app_id: String,
    pub version: Version,
    pub channel: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

pub fn handle_velopack_startup() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(first) = args.first() {
        let lower = first.to_ascii_lowercase();
        if HOOK_CLI_FLAGS.iter().any(|flag| *flag == lower) {
            exit(0);
        }
    }

    let restarted = std::env::var(HOOK_ENV_RESTART).is_ok();
    if restarted {
        return;
    }

    if let Some(install) = locate_install() {
        if let Some((pkg_path, _ver)) = find_pending_package(&install) {
            if spawn_update_apply(&install, &pkg_path, true).is_ok() {
                exit(0);
            }
        }
    }
}

pub fn update_source_url() -> String {
    std::env::var("TIDENATIVE_UPDATE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_UPDATE_SOURCE_URL.to_string())
}

pub fn locate_install() -> Option<VelopackInstall> {
    let exe_path = std::env::current_exe().ok()?;
    let current_dir = exe_path.parent()?;
    let root_dir = current_dir.parent()?;
    let update_exe = root_dir.join("Update.exe");
    let sq_version_path = current_dir.join("sq.version");

    if !update_exe.is_file() || !sq_version_path.is_file() {
        return None;
    }

    let xml = fs::read_to_string(&sq_version_path).ok()?;
    let (app_id, version, channel) = parse_sq_version_xml(&xml)?;
    let packages_dir = root_dir.join("packages");

    Some(VelopackInstall {
        root_dir: root_dir.to_path_buf(),
        packages_dir,
        update_exe,
        app_id,
        version,
        channel,
    })
}

pub fn parse_sq_version_xml(xml: &str) -> Option<(String, Version, String)> {
    let app_id = extract_xml_tag(xml, "id")?;
    let version_str = extract_xml_tag(xml, "version")?;
    let version = Version::parse(version_str.trim()).ok()?;
    let channel = extract_xml_tag(xml, "channel")
        .filter(|c| !c.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CHANNEL.to_string());
    Some((app_id, version, channel))
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_string())
}

fn parse_nupkg_version(file_name: &str, app_id: &str) -> Option<(Version, bool)> {
    let lower = file_name.to_ascii_lowercase();
    if !lower.ends_with(".nupkg") {
        return None;
    }
    let stem = &file_name[..file_name.len() - ".nupkg".len()];
    let (without_kind, is_full) = if let Some(s) = stem.strip_suffix("-full") {
        (s, true)
    } else if let Some(s) = stem.strip_suffix("-delta") {
        (s, false)
    } else {
        return None;
    };

    let prefix = format!("{app_id}-");
    let ver_str = if without_kind.len() > prefix.len()
        && without_kind[..prefix.len()].eq_ignore_ascii_case(&prefix)
    {
        &without_kind[prefix.len()..]
    } else {
        return None;
    };

    Version::parse(ver_str).ok().map(|v| (v, is_full))
}

fn find_local_full_packages(install: &VelopackInstall) -> Vec<(PathBuf, Version)> {
    let Ok(entries) = fs::read_dir(&install.packages_dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some((ver, true)) = parse_nupkg_version(name, &install.app_id) {
            found.push((path, ver));
        }
    }
    found
}

fn find_pending_package(install: &VelopackInstall) -> Option<(PathBuf, Version)> {
    find_local_full_packages(install)
        .into_iter()
        .filter(|(_, v)| *v > install.version)
        .max_by(|(_, a), (_, b)| a.cmp(b))
}

fn find_base_package(install: &VelopackInstall) -> Option<(PathBuf, Version)> {
    let locals = find_local_full_packages(install);
    locals
        .iter()
        .find(|(_, v)| *v == install.version)
        .cloned()
        .or_else(|| {
            locals
                .into_iter()
                .filter(|(_, v)| *v <= install.version)
                .max_by(|(_, a), (_, b)| a.cmp(b))
        })
}

fn not_installed_info(source_url: String) -> AppUpdateInfo {
    AppUpdateInfo {
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        is_installed: false,
        update_available: false,
        pending_restart: false,
        available_version: None,
        release_notes: None,
        is_delta: false,
        download_size_bytes: None,
        source_url,
    }
}

fn info_from_plan(
    install: &VelopackInstall,
    plan: &PendingUpdatePlan,
    pending_restart: bool,
) -> AppUpdateInfo {
    let is_delta = plan.base_package_path.is_some() && !plan.deltas.is_empty();
    let download_size_bytes = if is_delta {
        Some(plan.deltas.iter().map(|d| d.Size).sum())
    } else if plan.target.Size > 0 {
        Some(plan.target.Size)
    } else {
        None
    };
    let release_notes = (!plan.target.NotesMarkdown.trim().is_empty())
        .then(|| plan.target.NotesMarkdown.clone());

    AppUpdateInfo {
        current_version: install.version.to_string(),
        is_installed: true,
        update_available: true,
        pending_restart,
        available_version: Some(plan.target.Version.clone()),
        release_notes,
        is_delta,
        download_size_bytes,
        source_url: plan.source_url.clone(),
    }
}

pub fn get_local_status(cached: Option<&PendingUpdatePlan>) -> AppUpdateInfo {
    let source_url = update_source_url();
    let Some(install) = locate_install() else {
        return not_installed_info(source_url);
    };

    if let Some((path, version)) = find_pending_package(&install) {
        let size = fs::metadata(&path).ok().map(|m| m.len());
        return AppUpdateInfo {
            current_version: install.version.to_string(),
            is_installed: true,
            update_available: true,
            pending_restart: true,
            available_version: Some(version.to_string()),
            release_notes: cached
                .and_then(|p| (!p.target.NotesMarkdown.trim().is_empty()).then(|| p.target.NotesMarkdown.clone())),
            is_delta: false,
            download_size_bytes: size,
            source_url,
        };
    }

    if let Some(plan) = cached {
        return info_from_plan(&install, plan, false);
    }

    AppUpdateInfo {
        current_version: install.version.to_string(),
        is_installed: true,
        update_available: false,
        pending_restart: false,
        available_version: None,
        release_notes: None,
        is_delta: false,
        download_size_bytes: None,
        source_url,
    }
}

fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("TideNative/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Impossible de créer le client HTTP : {e}"))
}

pub fn parse_github_repo(source_url: &str) -> Option<(String, String)> {
    let trimmed = source_url.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))?;
    let mut parts = rest.split('/').filter(|s| !s.is_empty());
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    Some((owner, repo))
}

async fn fetch_release_feed(source_url: &str, channel: &str) -> Result<Vec<VelopackAsset>, String> {
    let releases_filename = format!("releases.{channel}.json");

    if !source_url.starts_with("http://") && !source_url.starts_with("https://") {
        let dir = Path::new(source_url);
        let feed_path = dir.join(&releases_filename);
        if !feed_path.is_file() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&feed_path)
            .map_err(|e| format!("Impossible de lire {}: {e}", feed_path.display()))?;
        let mut feed: VelopackAssetFeed = serde_json::from_str(&raw)
            .map_err(|e| format!("Format releases JSON invalide : {e}"))?;
        for asset in &mut feed.Assets {
            asset.download_url = Some(dir.join(&asset.FileName).to_string_lossy().to_string());
        }
        return Ok(feed.Assets);
    }

    let client = build_http_client()?;

    if let Some((owner, repo)) = parse_github_repo(source_url) {
        let api_url = format!("https://api.github.com/repos/{owner}/{repo}/releases?per_page=10");
        let resp = client
            .get(&api_url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| format!("Erreur réseau GitHub Releases : {e}"))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !resp.status().is_success() {
            return Err(format!("GitHub Releases a répondu HTTP {}", resp.status()));
        }

        let releases: Vec<GithubRelease> = resp
            .json()
            .await
            .map_err(|e| format!("Réponse GitHub Releases invalide : {e}"))?;

        let mut all_assets = Vec::new();
        for release in releases.into_iter().filter(|r| !r.draft && !r.prerelease) {
            let Some(feed_asset) = release
                .assets
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(&releases_filename))
            else {
                continue;
            };

            let url_map: HashMap<String, String> = release
                .assets
                .iter()
                .map(|a| (a.name.to_ascii_lowercase(), a.browser_download_url.clone()))
                .collect();

            let Ok(feed_resp) = client.get(&feed_asset.browser_download_url).send().await else {
                continue;
            };
            if !feed_resp.status().is_success() {
                continue;
            }
            let Ok(mut feed) = feed_resp.json::<VelopackAssetFeed>().await else {
                continue;
            };
            for asset in &mut feed.Assets {
                asset.download_url = url_map.get(&asset.FileName.to_ascii_lowercase()).cloned();
            }
            all_assets.extend(feed.Assets);
        }
        return Ok(all_assets);
    }

    let base = source_url.trim_end_matches('/');
    let feed_url = format!("{base}/{releases_filename}");
    let resp = client
        .get(&feed_url)
        .send()
        .await
        .map_err(|e| format!("Erreur réseau lors de la vérification : {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if !resp.status().is_success() {
        return Err(format!("Serveur de mise à jour HTTP {}", resp.status()));
    }
    let mut feed: VelopackAssetFeed = resp
        .json()
        .await
        .map_err(|e| format!("Flux releases JSON invalide : {e}"))?;
    for asset in &mut feed.Assets {
        asset.download_url = Some(format!("{base}/{}", asset.FileName));
    }
    Ok(feed.Assets)
}

pub fn select_update_plan(
    install: &VelopackInstall,
    assets: &[VelopackAsset],
    source_url: String,
) -> Option<PendingUpdatePlan> {
    let mut latest: Option<(&VelopackAsset, Version)> = None;
    for asset in assets {
        if !asset.Type.eq_ignore_ascii_case("Full") {
            continue;
        }
        let Ok(ver) = Version::parse(&asset.Version) else {
            continue;
        };
        if ver <= install.version {
            continue;
        }
        if latest.as_ref().is_none_or(|(_, best)| ver > *best) {
            latest = Some((asset, ver));
        }
    }

    let (target_asset, target_ver) = latest?;
    let mut base_package_path = None;
    let mut deltas = Vec::new();

    if let Some((base_path, base_ver)) = find_base_package(install) {
        let mut chain: Vec<(VelopackAsset, Version)> = assets
            .iter()
            .filter(|a| a.Type.eq_ignore_ascii_case("Delta"))
            .filter_map(|a| Version::parse(&a.Version).ok().map(|v| (a.clone(), v)))
            .filter(|(_, v)| *v > base_ver && *v <= target_ver)
            .collect();
        chain.sort_by(|(_, a), (_, b)| a.cmp(b));
        chain.dedup_by(|(_, a), (_, b)| a == b);

        let reaches_target = chain.last().is_some_and(|(_, v)| *v == target_ver);
        if reaches_target && chain.len() <= 10 {
            base_package_path = Some(base_path);
            deltas = chain.into_iter().map(|(a, _)| a).collect();
        }
    }

    Some(PendingUpdatePlan {
        target: target_asset.clone(),
        base_package_path,
        deltas,
        source_url,
    })
}

pub async fn check_remote_updates() -> Result<(AppUpdateInfo, Option<PendingUpdatePlan>), String> {
    let source_url = update_source_url();
    let Some(install) = locate_install() else {
        return Ok((not_installed_info(source_url), None));
    };

    if let Some((path, version)) = find_pending_package(&install) {
        let size = fs::metadata(&path).ok().map(|m| m.len());
        return Ok((
            AppUpdateInfo {
                current_version: install.version.to_string(),
                is_installed: true,
                update_available: true,
                pending_restart: true,
                available_version: Some(version.to_string()),
                release_notes: None,
                is_delta: false,
                download_size_bytes: size,
                source_url,
            },
            None,
        ));
    }

    let assets = fetch_release_feed(&source_url, &install.channel).await?;
    if let Some(plan) = select_update_plan(&install, &assets, source_url.clone()) {
        let info = info_from_plan(&install, &plan, false);
        Ok((info, Some(plan)))
    } else {
        Ok((
            AppUpdateInfo {
                current_version: install.version.to_string(),
                is_installed: true,
                update_available: false,
                pending_restart: false,
                available_version: None,
                release_notes: None,
                is_delta: false,
                download_size_bytes: None,
                source_url,
            },
            None,
        ))
    }
}

fn safe_file_name(name: &str) -> String {
    Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string())
}

fn verify_file_integrity(path: &Path, asset: &VelopackAsset) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| format!("Fichier téléchargé illisible : {e}"))?;
    if asset.Size > 0 && meta.len() != asset.Size {
        return Err(format!(
            "Taille invalide pour {} (attendu {} o, reçu {} o)",
            asset.FileName,
            asset.Size,
            meta.len()
        ));
    }

    if !asset.SHA256.is_empty() {
        let mut file = File::open(path).map_err(|e| format!("Ouverture SHA256 échouée : {e}"))?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf).map_err(|e| format!("Lecture SHA256 échouée : {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02X}")).collect();
        if !hex.eq_ignore_ascii_case(&asset.SHA256) {
            return Err(format!("Empreinte SHA256 invalide pour {}", asset.FileName));
        }
    }
    Ok(())
}

async fn download_asset_to_file<F>(
    client: &reqwest::Client,
    asset: &VelopackAsset,
    dest: &Path,
    mut on_progress: F,
) -> Result<(), String>
where
    F: FnMut(i16),
{
    let Some(src) = asset.download_url.as_deref() else {
        return Err(format!("URL de téléchargement manquante pour {}", asset.FileName));
    };

    if !src.starts_with("http://") && !src.starts_with("https://") {
        fs::copy(src, dest).map_err(|e| format!("Copie locale échouée ({src}) : {e}"))?;
        on_progress(100);
        return Ok(());
    }

    let mut resp = client
        .get(src)
        .send()
        .await
        .map_err(|e| format!("Téléchargement échoué ({}) : {e}", asset.FileName))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Téléchargement {} refusé (HTTP {})",
            asset.FileName,
            resp.status()
        ));
    }

    let total = resp.content_length().or((asset.Size > 0).then_some(asset.Size));
    let mut file = File::create(dest).map_err(|e| format!("Création fichier temporaire échouée : {e}"))?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("Flux de téléchargement interrompu : {e}"))?
    {
        file.write_all(&chunk)
            .map_err(|e| format!("Écriture disque échouée : {e}"))?;
        downloaded += chunk.len() as u64;
        if let Some(total_bytes) = total.filter(|t| *t > 0) {
            let pct = ((downloaded.saturating_mul(100)) / total_bytes).min(100) as i16;
            on_progress(pct);
        }
    }
    file.flush().map_err(|e| format!("Flush disque échoué : {e}"))?;
    on_progress(100);
    Ok(())
}

pub async fn download_update_plan<F>(
    plan: &PendingUpdatePlan,
    mut on_progress: F,
) -> Result<AppUpdateInfo, String>
where
    F: FnMut(i16),
{
    let install = locate_install()
        .ok_or_else(|| "TideNative n'est pas installé via l'installateur Velopack.".to_string())?;
    fs::create_dir_all(&install.packages_dir)
        .map_err(|e| format!("Impossible de créer le dossier packages : {e}"))?;

    let final_target = install.packages_dir.join(safe_file_name(&plan.target.FileName));
    if final_target.is_file() && verify_file_integrity(&final_target, &plan.target).is_ok() {
        on_progress(100);
        return Ok(info_from_plan(&install, plan, true));
    }

    let partial_target = final_target.with_extension("partial");
    let _ = fs::remove_file(&partial_target);

    let client = reqwest::Client::builder()
        .user_agent(format!("TideNative/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("Impossible de créer le client HTTP : {e}"))?;

    let mut delta_succeeded = false;
    if let (Some(base_path), false) = (&plan.base_package_path, plan.deltas.is_empty()) {
        let mut delta_files = Vec::new();
        let mut all_downloaded = true;
        let count = plan.deltas.len() as f64;

        for (idx, delta) in plan.deltas.iter().enumerate() {
            let delta_path = install.packages_dir.join(safe_file_name(&delta.FileName));
            let delta_partial = delta_path.with_extension("partial");
            let start = (idx as f64 / count) * 70.0;
            let end = ((idx + 1) as f64 / count) * 70.0;

            let dl_ok = download_asset_to_file(&client, delta, &delta_partial, |p| {
                let scaled = (start + (end - start) * (p as f64 / 100.0)) as i16;
                on_progress(scaled.clamp(0, 70));
            })
            .await
            .and_then(|_| verify_file_integrity(&delta_partial, delta))
            .and_then(|_| {
                fs::rename(&delta_partial, &delta_path)
                    .map_err(|e| format!("Renommage delta échoué : {e}"))
            });

            if dl_ok.is_err() {
                let _ = fs::remove_file(&delta_partial);
                all_downloaded = false;
                break;
            }
            delta_files.push(delta_path);
        }

        if all_downloaded {
            on_progress(75);
            let mut args: Vec<OsString> = vec![
                "patch".into(),
                "--old".into(),
                base_path.clone().into(),
                "--output".into(),
                partial_target.clone().into(),
            ];
            for df in &delta_files {
                args.push("--delta".into());
                args.push(df.clone().into());
            }

            let update_exe = install.update_exe.clone();
            let patch_res = tokio::task::spawn_blocking(move || {
                Command::new(&update_exe).args(&args).output()
            })
            .await;

            if let Ok(Ok(out)) = patch_res {
                if out.status.success() && verify_file_integrity(&partial_target, &plan.target).is_ok() {
                    delta_succeeded = true;
                    on_progress(100);
                }
            }
            for df in &delta_files {
                let _ = fs::remove_file(df);
            }
        }
    }

    if !delta_succeeded {
        let _ = fs::remove_file(&partial_target);
        download_asset_to_file(&client, &plan.target, &partial_target, |p| {
            on_progress(p.clamp(0, 100));
        })
        .await?;
        verify_file_integrity(&partial_target, &plan.target)?;
    }

    fs::rename(&partial_target, &final_target)
        .map_err(|e| format!("Finalisation du paquet échouée : {e}"))?;

    Ok(info_from_plan(&install, plan, true))
}

fn spawn_update_apply(
    install: &VelopackInstall,
    package_path: &Path,
    restart: bool,
) -> Result<(), String> {
    let mut args: Vec<OsString> = vec![
        "apply".into(),
        "--package".into(),
        package_path.into(),
        "--waitPid".into(),
        std::process::id().to_string().into(),
    ];
    if !restart {
        args.push("--norestart".into());
    }
    args.push("--root".into());
    args.push(install.root_dir.clone().into());
    args.push("--packageDir".into());
    args.push(install.packages_dir.clone().into());

    Command::new(&install.update_exe)
        .args(&args)
        .current_dir(&install.root_dir)
        .spawn()
        .map_err(|e| format!("Impossible de lancer Update.exe : {e}"))?;
    Ok(())
}

pub fn schedule_apply_and_restart(cached: Option<&PendingUpdatePlan>) -> Result<(), String> {
    let install = locate_install()
        .ok_or_else(|| "TideNative n'est pas installé via l'installateur Velopack.".to_string())?;

    if let Some((pkg_path, _)) = find_pending_package(&install) {
        return spawn_update_apply(&install, &pkg_path, true);
    }

    if let Some(plan) = cached {
        let candidate = install.packages_dir.join(safe_file_name(&plan.target.FileName));
        if candidate.is_file() {
            return spawn_update_apply(&install, &candidate, true);
        }
    }

    Err("Aucune mise à jour téléchargée prête à être installée.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sq_version_manifest_xml() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2010/07/nuspec.xsd">
  <metadata>
    <id>TideNative</id>
    <version>0.1.0</version>
    <channel>win</channel>
  </metadata>
</package>"#;
        let (id, version, channel) = parse_sq_version_xml(xml).expect("parse sq.version");
        assert_eq!(id, "TideNative");
        assert_eq!(version, Version::new(0, 1, 0));
        assert_eq!(channel, "win");
    }

    #[test]
    fn parses_github_repo_urls() {
        assert_eq!(
            parse_github_repo("https://github.com/Leboxis/TideNative"),
            Some(("Leboxis".to_string(), "TideNative".to_string()))
        );
        assert_eq!(
            parse_github_repo("https://github.com/Leboxis/TideNative.git"),
            Some(("Leboxis".to_string(), "TideNative".to_string()))
        );
        assert_eq!(parse_github_repo("https://updates.example.com"), None);
    }

    #[test]
    fn selects_newest_full_release_above_installed_version() {
        let install = VelopackInstall {
            root_dir: PathBuf::from("C:/App"),
            packages_dir: PathBuf::from("C:/App/packages"),
            update_exe: PathBuf::from("C:/App/Update.exe"),
            app_id: "TideNative".to_string(),
            version: Version::new(0, 1, 0),
            channel: "win".to_string(),
        };
        let assets = vec![
            VelopackAsset {
                PackageId: "TideNative".into(),
                Version: "0.1.0".into(),
                Type: "Full".into(),
                FileName: "TideNative-0.1.0-full.nupkg".into(),
                SHA1: String::new(),
                SHA256: String::new(),
                Size: 1000,
                NotesMarkdown: String::new(),
                download_url: Some("https://example.com/0.1.0".into()),
            },
            VelopackAsset {
                PackageId: "TideNative".into(),
                Version: "0.2.0".into(),
                Type: "Full".into(),
                FileName: "TideNative-0.2.0-full.nupkg".into(),
                SHA1: String::new(),
                SHA256: String::new(),
                Size: 2000,
                NotesMarkdown: "Nouveautés 0.2.0".into(),
                download_url: Some("https://example.com/0.2.0".into()),
            },
        ];

        let plan = select_update_plan(&install, &assets, DEFAULT_UPDATE_SOURCE_URL.into())
            .expect("select 0.2.0 update");
        assert_eq!(plan.target.Version, "0.2.0");
        assert_eq!(plan.target.NotesMarkdown, "Nouveautés 0.2.0");
    }
}
