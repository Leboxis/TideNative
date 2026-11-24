//! Persistance du token dans le Gestionnaire d'identification Windows
//! (Credential Manager, type GENERIC, persistance LOCAL_MACHINE = profil local).
//!
//! Le blob (JSON UTF-8) contient access + refresh token : on peut donc
//! afficher l'UI et appeler l'API au lancement sans aller-retour réseau tant
//! que l'access_token est valide — important pour le démarrage instantané.
//!
//! Le même mécanisme sert de coffre générique pour les autres secrets de
//! l'application (Last.fm, etc.) via [`save_named_secret`] /
//! [`load_named_secret`] / [`delete_named_secret`], rangés sous le service
//! `TideNative`.

use crate::error::{AuthError, Result};
use crate::token::TokenSet;

/// Nom de la cible visible dans « Gestionnaire d'identification ».
pub const TARGET: &str = "TideNative/tidal";
/// Service Credential Manager commun à tous les secrets nommés.
pub const SERVICE: &str = "TideNative";
/// CRED_MAX_CREDENTIAL_BLOB_SIZE = 5 * 512 octets.
const MAX_BLOB: usize = 5 * 512;

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{
        CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    };
    use windows::core::{HSTRING, PWSTR};

    fn store_err(e: windows::core::Error) -> AuthError {
        AuthError::Store(e.message().to_string())
    }

    fn is_not_found(e: &windows::core::Error) -> bool {
        e.code() == ERROR_NOT_FOUND.to_hresult()
    }

    /// Écrit un blob brut sous `target_name` ; le buffer est effacé après l'appel.
    fn write_blob(target_name: &str, user_name: &str, mut blob: Vec<u8>) -> Result<()> {
        if blob.len() > MAX_BLOB {
            return Err(AuthError::Store(format!(
                "blob trop grand ({} > {MAX_BLOB} octets)",
                blob.len()
            )));
        }
        let mut target: Vec<u16> = target_name.encode_utf16().chain(Some(0)).collect();
        let mut user: Vec<u16> = user_name.encode_utf16().chain(Some(0)).collect();
        let cred = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: PWSTR(user.as_mut_ptr()),
            ..Default::default()
        };
        // SAFETY: `cred` et les buffers référencés vivent jusqu'au retour de l'appel.
        let res = unsafe { CredWriteW(&cred, 0) };
        blob.fill(0); // efface la copie en clair
        res.map_err(store_err)
    }

    /// Lit le blob brut de `target_name` (`None` si la cible n'existe pas).
    fn read_blob(target_name: &str) -> Result<Option<Vec<u8>>> {
        let mut pcred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: CredReadW alloue `pcred`, libéré par CredFree ci-dessous.
        match unsafe { CredReadW(&HSTRING::from(target_name), CRED_TYPE_GENERIC, None, &mut pcred) } {
            Ok(()) => {}
            Err(e) if is_not_found(&e) => return Ok(None),
            Err(e) => return Err(store_err(e)),
        }
        // SAFETY: pointeur valide renvoyé par CredReadW ; le blob fait
        // `CredentialBlobSize` octets. On copie avant de libérer.
        let bytes = unsafe {
            let c = &*pcred;
            let slice = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
            let owned = slice.to_vec();
            CredFree(pcred as *const _);
            owned
        };
        Ok(Some(bytes))
    }

    pub fn save(token: &TokenSet) -> Result<()> {
        save_at(TARGET, token)
    }

    pub(crate) fn save_at(target_name: &str, token: &TokenSet) -> Result<()> {
        write_blob(target_name, "tidal", serde_json::to_vec(token)?)
    }

    pub fn load() -> Result<Option<TokenSet>> {
        load_at(TARGET)
    }

    pub(crate) fn load_at(target_name: &str) -> Result<Option<TokenSet>> {
        match read_blob(target_name)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn delete() -> Result<()> {
        delete_at(TARGET)
    }

    pub(crate) fn delete_at(target_name: &str) -> Result<()> {
        // SAFETY: appel FFI simple avec une chaîne terminée par NUL.
        match unsafe { CredDeleteW(&HSTRING::from(target_name), CRED_TYPE_GENERIC, None) } {
            Ok(()) => Ok(()),
            Err(e) if is_not_found(&e) => Ok(()),
            Err(e) => Err(store_err(e)),
        }
    }

    pub(crate) fn save_secret_at(target_name: &str, user_name: &str, secret: &[u8]) -> Result<()> {
        write_blob(target_name, user_name, secret.to_vec())
    }

    pub(crate) fn load_secret_at(target_name: &str) -> Result<Option<Vec<u8>>> {
        read_blob(target_name)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub fn save(_: &TokenSet) -> Result<()> {
        Err(AuthError::Store("non supporté hors Windows".into()))
    }
    pub fn load() -> Result<Option<TokenSet>> {
        Ok(None)
    }
    pub fn delete() -> Result<()> {
        Ok(())
    }
    pub(crate) fn save_secret_at(_: &str, _: &str, _: &[u8]) -> Result<()> {
        Err(AuthError::Store("non supporté hors Windows".into()))
    }
    pub(crate) fn load_secret_at(_: &str) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
    pub(crate) fn delete_at(_: &str) -> Result<()> {
        Ok(())
    }
}

pub use imp::{delete, load, save};

/// Cible Credential Manager d'un secret nommé (`TideNative/<compte>`).
fn secret_target(account: &str) -> Result<String> {
    let account = account.trim();
    if account.is_empty() {
        return Err(AuthError::Store("nom de secret vide".into()));
    }
    Ok(format!("{SERVICE}/{account}"))
}

/// Enregistre un secret texte libre dans le Gestionnaire d'identification
/// Windows (type GENERIC, persistance profil local) sous `TideNative/<account>`.
/// Une valeur existante est remplacée.
pub fn save_named_secret(account: &str, value: &str) -> Result<()> {
    imp::save_secret_at(&secret_target(account)?, account.trim(), value.as_bytes())
}

/// Lit un secret texte (`None` s'il n'existe pas encore).
pub fn load_named_secret(account: &str) -> Result<Option<String>> {
    let Some(bytes) = imp::load_secret_at(&secret_target(account)?)? else {
        return Ok(None);
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|e| AuthError::Store(format!("secret non UTF-8 : {e}")))
}

/// Supprime un secret (idempotent : une cible absente est un succès).
pub fn delete_named_secret(account: &str) -> Result<()> {
    imp::delete_at(&secret_target(account)?)
}

#[cfg(all(test, windows))]
mod tests {
    use super::imp::{delete_at, load_at, save_at};
    use super::{delete_named_secret, load_named_secret, save_named_secret, secret_target};
    use crate::token::TokenSet;

    #[test]
    fn windows_credential_manager_roundtrip() {
        const TEST_TARGET: &str = "TideNative/tidal-unit-test";
        let _ = delete_at(TEST_TARGET);
        let sample = TokenSet {
            token_type: "Bearer".into(),
            access_token: "access-xyz".into(),
            refresh_token: "refresh-abc".into(),
            expires_at: 4_102_444_800,
            is_pkce: true,
            user_id: Some(42),
            country_code: Some("FR".into()),
        };
        save_at(TEST_TARGET, &sample).unwrap();
        let loaded = load_at(TEST_TARGET).unwrap().expect("credential should exist");
        assert_eq!(loaded.access_token, "access-xyz");
        assert_eq!(loaded.refresh_token, "refresh-abc");
        assert!(loaded.is_pkce);
        assert_eq!(loaded.user_id, Some(42));
        assert_eq!(loaded.country_code.as_deref(), Some("FR"));
        delete_at(TEST_TARGET).unwrap();
        assert!(load_at(TEST_TARGET).unwrap().is_none());
    }

    #[test]
    fn named_secret_roundtrip_save_load_and_delete() {
        let account = format!(
            "named-secret-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let target = secret_target(&account).expect("cible de test valide");
        let _ = delete_at(&target);
        assert!(load_named_secret(&account).unwrap().is_none());

        save_named_secret(&account, "s3cr3t-✓").unwrap();
        assert_eq!(
            load_named_secret(&account).unwrap().as_deref(),
            Some("s3cr3t-✓")
        );

        // Une nouvelle écriture remplace la valeur précédente.
        save_named_secret(&account, "rotated").unwrap();
        assert_eq!(load_named_secret(&account).unwrap().as_deref(), Some("rotated"));

        delete_named_secret(&account).unwrap();
        assert!(load_named_secret(&account).unwrap().is_none());
        // Suppression idempotente.
        delete_named_secret(&account).unwrap();
    }

    #[test]
    fn named_secret_rejects_empty_account() {
        assert!(save_named_secret("   ", "x").is_err());
        assert!(load_named_secret("").is_err());
        assert!(delete_named_secret("").is_err());
    }
}
