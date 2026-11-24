//! Utilitaires système Windows : lecture du presse-papiers UTF-16 (`CF_UNICODETEXT`)
//! et ouverture de dossiers / URLs exclusivement via la crate `open` + `dunce`.

use crate::{PlatformError, Result};
use std::path::Path;

/// Lit le texte Unicode actuellement présent dans le presse-papiers Windows.
#[cfg(windows)]
pub fn read_clipboard_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    unsafe {
        if IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)).is_err() {
            return None;
        }
        if OpenClipboard(None).is_err() {
            return None;
        }
        let result = (|| {
            let handle = GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok()?;
            let hglobal = HGLOBAL(handle.0);
            let ptr = GlobalLock(hglobal) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
                if len > 65_536 {
                    break;
                }
            }
            let slice = std::slice::from_raw_parts(ptr, len);
            let text = String::from_utf16_lossy(slice);
            let _ = GlobalUnlock(hglobal);
            Some(text)
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(not(windows))]
pub fn read_clipboard_text() -> Option<String> {
    None
}

/// Écrit du texte Unicode dans le presse-papiers Windows (`CF_UNICODETEXT`).
#[cfg(windows)]
pub fn write_clipboard_text(text: &str) -> Result<()> {
    use windows::Win32::Foundation::{GlobalFree, HANDLE};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    utf16.push(0);
    let byte_len = utf16.len() * std::mem::size_of::<u16>();

    unsafe {
        OpenClipboard(None)
            .map_err(|e| PlatformError::Shell(format!("OpenClipboard : {e}")))?;
        let outcome = (|| -> Result<()> {
            EmptyClipboard()
                .map_err(|e| PlatformError::Shell(format!("EmptyClipboard : {e}")))?;
            let hglobal = GlobalAlloc(GMEM_MOVEABLE, byte_len)
                .map_err(|e| PlatformError::Shell(format!("GlobalAlloc : {e}")))?;
            let ptr = GlobalLock(hglobal) as *mut u16;
            if ptr.is_null() {
                let _ = GlobalFree(Some(hglobal));
                return Err(PlatformError::Shell("GlobalLock a retourné null".into()));
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
            let _ = GlobalUnlock(hglobal);
            if let Err(e) = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(hglobal.0))) {
                let _ = GlobalFree(Some(hglobal));
                return Err(PlatformError::Shell(format!("SetClipboardData : {e}")));
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        outcome
    }
}

#[cfg(not(windows))]
pub fn write_clipboard_text(_text: &str) -> Result<()> {
    Ok(())
}

/// Ouvre une URL dans le navigateur par défaut exclusivement via `open::that`.
pub fn open_url_in_browser(url: &str) -> Result<()> {
    open::that(url).map_err(|e| PlatformError::Shell(format!("open::that({url}) : {e}")))
}

/// Ouvre un dossier local dans le gestionnaire de fichiers système exclusivement via `open::that`
/// après création (`create_dir_all`) et normalisation sans préfixe `\\?\` (`dunce::canonicalize`).
pub fn open_folder_in_explorer(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| {
        PlatformError::Shell(format!("impossible de créer {} : {e}", path.display()))
    })?;
    let clean = tidal_core::paths::normalize_path(path);
    open::that(&clean).map_err(|e| {
        PlatformError::Shell(format!("impossible d'ouvrir {} : {e}", clean.display()))
    })
}
