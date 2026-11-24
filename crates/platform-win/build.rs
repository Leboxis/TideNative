use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        other => panic!("Unsupported Windows arch for WebView2Loader.dll: {other:?}"),
    };

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let dest = out_dir.join("WebView2Loader.dll");

    let src = find_webview2_loader_dll(&out_dir, arch)
        .expect("Impossible de localiser WebView2Loader.dll depuis webview2-com-sys");

    fs::copy(&src, &dest).unwrap_or_else(|e| {
        panic!(
            "Échec copie {} -> {} : {e}",
            src.display(),
            dest.display()
        )
    });
}

fn find_webview2_loader_dll(out_dir: &Path, arch: &str) -> Option<PathBuf> {
    // 1. Recherche dans le dossier `build/` du profil Cargo courant
    //    (Cargo exécute `webview2-com-sys/build.rs` avant `tidal-platform-win/build.rs`)
    for ancestor in out_dir.ancestors() {
        if ancestor.file_name().and_then(|n| n.to_str()) == Some("build") {
            if let Some(found) = search_build_dir(ancestor, arch) {
                return Some(found);
            }
            break;
        }
    }

    // 2. Fallback : registre Cargo (`$CARGO_HOME/registry/src/*/webview2-com-sys-*/<arch>/WebView2Loader.dll`)
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".cargo")))?;
    let reg_src = cargo_home.join("registry").join("src");
    for index_dir in fs::read_dir(&reg_src).ok()?.flatten() {
        for crate_dir in fs::read_dir(index_dir.path()).ok()?.flatten() {
            let name = crate_dir.file_name();
            if name.to_string_lossy().starts_with("webview2-com-sys-") {
                let candidate = crate_dir.path().join(arch).join("WebView2Loader.dll");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn search_build_dir(build_dir: &Path, arch: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(build_dir).ok()?.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "webview2-com-sys" {
            // Layout Cargo récent : build/webview2-com-sys/<hash>/out/<arch>/WebView2Loader.dll
            for hash_dir in fs::read_dir(entry.path()).ok()?.flatten() {
                let candidate = hash_dir
                    .path()
                    .join("out")
                    .join(arch)
                    .join("WebView2Loader.dll");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        } else if name_str.starts_with("webview2-com-sys-") {
            // Layout Cargo classique : build/webview2-com-sys-<hash>/out/<arch>/WebView2Loader.dll
            let candidate = entry
                .path()
                .join("out")
                .join(arch)
                .join("WebView2Loader.dll");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}
