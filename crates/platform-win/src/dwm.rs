//! Habillage natif Windows 11 DWM : mode sombre immersif, coins arrondis,
//! couleur de barre de titre accordée au thème audiophile `#0B0D14` et effet **Mica**.

use crate::{PlatformError, Result};

/// Résultat détaillé de l'application des attributs DWM Windows 11.
#[derive(Debug, Clone, Copy, Default)]
pub struct DwmBackdropReport {
    pub dark_mode_applied: bool,
    pub mica_applied: bool,
    pub caption_tinted: bool,
}

#[cfg(windows)]
pub fn apply_windows11_styling(hwnd_raw: isize, enable_mica: bool) -> Result<DwmBackdropReport> {
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_SYSTEMBACKDROP_TYPE,
        DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DWM_SYSTEMBACKDROP_TYPE, DwmSetWindowAttribute,
    };

    if hwnd_raw == 0 {
        return Err(PlatformError::InvalidHandle);
    }
    let hwnd = HWND(hwnd_raw as *mut c_void);
    let mut report = DwmBackdropReport::default();

    // 1. Mode sombre immersif Windows 10 20H1+ / Windows 11
    let dark: i32 = 1;
    if unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const c_void,
            size_of::<i32>() as u32,
        )
    }
    .is_ok()
    {
        report.dark_mode_applied = true;
    }

    // 2. Coins arrondis natifs Windows 11
    let corner = DWMWCP_ROUND;
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const c_void,
            size_of::<i32>() as u32,
        )
    };

    // 3. Barre de titre & bordure assorties au fond audiophile `#0B0D14` / `#242B40` (COLORREF = 0x00BBGGRR)
    let caption_color: u32 = 0x0014_0D0B;
    if unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption_color as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        )
    }
    .is_ok()
    {
        report.caption_tinted = true;
    }

    let border_color: u32 = 0x0040_2B24;
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border_color as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        )
    };

    // 4. Effet Mica Windows 11 22H2+ (`DWMSBT_MAINWINDOW = 2`)
    if enable_mica {
        let backdrop = DWM_SYSTEMBACKDROP_TYPE(2); // DWMSBT_MAINWINDOW
        if unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const _ as *const c_void,
                size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
            )
        }
        .is_ok()
        {
            report.mica_applied = true;
        }
    }

    Ok(report)
}

#[cfg(not(windows))]
pub fn apply_windows11_styling(_hwnd_raw: isize, _enable_mica: bool) -> Result<DwmBackdropReport> {
    Ok(DwmBackdropReport::default())
}

/// En mode compact, conserve les coins arrondis natifs DWM Windows 11 et masque
/// la bordure DWM (`DWMWA_COLOR_NONE = 0xFFFFFFFE`) pour éviter tout liseré ou
/// bordure GDI parasite autour du mini-lecteur.
#[cfg(windows)]
pub fn apply_compact_dwm_styling(hwnd_raw: isize) -> Result<()> {
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };

    if hwnd_raw == 0 {
        return Err(PlatformError::InvalidHandle);
    }
    let hwnd = HWND(hwnd_raw as *mut c_void);
    let corner = DWMWCP_ROUND;
    let border_none: u32 = 0xFFFF_FFFE;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const c_void,
            size_of::<i32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border_none as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        );
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn apply_compact_dwm_styling(_hwnd_raw: isize) -> Result<()> {
    Ok(())
}
