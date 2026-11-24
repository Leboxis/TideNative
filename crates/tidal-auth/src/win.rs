//! Petites intégrations système natives (ouverture d'URL via la crate `open`).

/// Ouvre une URL web dans le navigateur par défaut via `open::that`.
pub fn open_in_browser(url: &str) -> bool {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return false; // n'ouvre jamais autre chose qu'une URL web
    }
    open::that(url).is_ok()
}
