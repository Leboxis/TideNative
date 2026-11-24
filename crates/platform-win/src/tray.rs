//! Icône de notification native Windows (zone de notification) et menu
//! contextuel Win32 : affichage de la fenêtre, contrôle de lecture et
//! fermeture propre de l'application.
//!
//! L'icône est portée par une fenêtre cachée créée sur un thread dédié
//! (`tidenative-tray`) qui pompe sa propre boucle de messages Win32. Le
//! `HICON` est construit depuis une image RGBA (zéro dépendance externe) et
//! toujours détruit au démontage : quitter TideNative ne laisse jamais
//! d'icône fantôme dans la barre des tâches.

use crate::Result;
use tokio::sync::mpsc::UnboundedSender;

/// Commande émise depuis l'icône de notification Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayCommand {
    ShowWindow,
    TogglePlayPause,
    Next,
    Previous,
    Quit,
}

#[cfg(windows)]
const TRAY_UID: u32 = 1;

/// Message privé envoyé par `Shell_NotifyIconW` à la fenêtre cachée.
#[cfg(windows)]
const TRAY_CALLBACK_MESSAGE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;

#[cfg(windows)]
const MENU_SHOW: u32 = 1001;
#[cfg(windows)]
const MENU_TOGGLE: u32 = 1002;
#[cfg(windows)]
const MENU_PREVIOUS: u32 = 1003;
#[cfg(windows)]
const MENU_NEXT: u32 = 1004;
#[cfg(windows)]
const MENU_QUIT: u32 = 1005;

// ---------------------------------------------------------------------------
// Implémentation Windows
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub struct TrayController {
    hwnd: isize,
    u_id: u32,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl TrayController {
    /// Crée l'icône de notification sur un thread dédié et attend que la
    /// fenêtre de messages Win32 soit prête.
    pub fn spawn(
        icon_rgba: Vec<u8>,
        icon_w: u32,
        icon_h: u32,
        cmd_tx: UnboundedSender<TrayCommand>,
    ) -> Result<Self> {
        use crate::PlatformError;

        let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_shutdown = std::sync::Arc::clone(&shutdown);
        let (init_tx, init_rx) =
            std::sync::mpsc::sync_channel::<std::result::Result<isize, String>>(1);

        let thread = std::thread::Builder::new()
            .name("tidenative-tray".into())
            .spawn(move || {
                if let Err(message) =
                    run_tray(icon_rgba, icon_w, icon_h, cmd_tx, &init_tx, &thread_shutdown)
                {
                    // Sans récepteur (initialisation déjà confirmée) l'envoi
                    // échoue immédiatement, sans blocage.
                    let _ = init_tx.send(Err(message));
                }
            })
            .map_err(|e| PlatformError::Tray(format!("thread « tidenative-tray » : {e}")))?;

        match init_rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self {
                hwnd,
                u_id: TRAY_UID,
                shutdown,
                thread: Some(thread),
            }),
            Ok(Err(message)) => {
                let _ = thread.join();
                Err(PlatformError::Tray(message))
            }
            Err(_) => {
                let _ = thread.join();
                Err(PlatformError::Tray(
                    "le thread de l'icône de notification s'est arrêté prématurément".into(),
                ))
            }
        }
    }

    /// Met à jour l'infobulle de l'icône (tronquée à 127 unités UTF-16).
    pub fn update_tooltip(&self, tip: &str) -> Result<()> {
        use std::ffi::c_void;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Shell::{NIF_TIP, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};

        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = HWND(self.hwnd as usize as *mut c_void);
        nid.uID = self.u_id;
        nid.uFlags = NIF_TIP;
        nid.szTip = tooltip_field(tip);
        let updated = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
        if updated.as_bool() {
            Ok(())
        } else {
            Err(crate::PlatformError::Tray(
                "Shell_NotifyIconW(NIM_MODIFY) a échoué".into(),
            ))
        }
    }
}

#[cfg(windows)]
impl Drop for TrayController {
    fn drop(&mut self) {
        use std::ffi::c_void;
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
        unsafe {
            let _ = PostMessageW(
                Some(HWND(self.hwnd as usize as *mut c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(windows)]
struct TrayState {
    cmd_tx: UnboundedSender<TrayCommand>,
}

#[cfg(windows)]
thread_local! {
    static TRAY_STATE: std::cell::RefCell<Option<TrayState>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(windows)]
fn send_command(command: TrayCommand) {
    TRAY_STATE.with(|slot| {
        if let Some(state) = slot.borrow().as_ref() {
            let _ = state.cmd_tx.send(command);
        }
    });
}

/// Initialise fenêtre cachée, icône et boucle de messages ; ne rend la main
/// qu'après `WM_QUIT` et le retrait complet de l'icône.
#[cfg(windows)]
fn run_tray(
    icon_rgba: Vec<u8>,
    icon_w: u32,
    icon_h: u32,
    cmd_tx: UnboundedSender<TrayCommand>,
    init_tx: &std::sync::mpsc::SyncSender<std::result::Result<isize, String>>,
    shutdown: &std::sync::atomic::AtomicBool,
) -> std::result::Result<(), String> {
    use windows::Win32::Foundation::HINSTANCE;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{
        NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyIcon, DestroyWindow, DispatchMessageW, GetMessageW, IDI_APPLICATION,
        LoadIconW, MSG, RegisterClassW, TranslateMessage, WINDOW_STYLE, WNDCLASSW,
        WS_EX_TOOLWINDOW,
    };
    use windows::core::{PCWSTR, w};

    TRAY_STATE.with(|slot| *slot.borrow_mut() = Some(TrayState { cmd_tx }));

    unsafe {
        let module =
            GetModuleHandleW(PCWSTR::null()).map_err(|e| format!("GetModuleHandleW : {e}"))?;
        let hinstance = HINSTANCE(module.0);
        let class_name = w!("TideNativeTrayMsgWnd");
        let class = WNDCLASSW {
            lpfnWndProc: Some(tray_wnd_proc),
            hInstance: hinstance,
            lpszClassName: class_name,
            ..Default::default()
        };
        // Un second contrôleur (tests) retrouve la classe déjà enregistrée :
        // l'échec éventuel est sans conséquence, `CreateWindowExW` tranche.
        let _ = RegisterClassW(&class);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name,
            w!("TideNative"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance),
            None,
        )
        .map_err(|e| format!("CreateWindowExW : {e}"))?;

        let (hicon, icon_owned) = match create_icon_from_rgba(&icon_rgba, icon_w, icon_h) {
            Some(icon) => (icon, true),
            None => (
                LoadIconW(None, IDI_APPLICATION)
                    .map_err(|e| format!("LoadIconW(IDI_APPLICATION) : {e}"))?,
                // Icône système partagée : ne jamais la détruire.
                false,
            ),
        };

        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_UID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = TRAY_CALLBACK_MESSAGE;
        nid.hIcon = hicon;
        nid.szTip = tooltip_field("TideNative");
        // Best effort : sans explorateur (session non interactive) l'icône
        // n'apparaît pas, la boucle de messages reste néanmoins fonctionnelle.
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        if init_tx.send(Ok(hwnd.0 as isize)).is_err() {
            remove_tray_icon(hwnd);
            if icon_owned {
                let _ = DestroyIcon(hicon);
            }
            let _ = DestroyWindow(hwnd);
            TRAY_STATE.with(|slot| *slot.borrow_mut() = None);
            return Ok(());
        }

        let mut msg = MSG::default();
        loop {
            if shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let received = GetMessageW(&mut msg, None, 0, 0);
            if received.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        remove_tray_icon(hwnd);
        if icon_owned {
            let _ = DestroyIcon(hicon);
        }
        let _ = DestroyWindow(hwnd);
        TRAY_STATE.with(|slot| *slot.borrow_mut() = None);
    }

    Ok(())
}

#[cfg(windows)]
fn remove_tray_icon(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::Shell::{NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW};

    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = TRAY_UID;
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

/// Convertit une image RGBA en `HICON` (BGRA + masque monochrome).
#[cfg(windows)]
unsafe fn create_icon_from_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::TRUE;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
        DeleteObject, GdiFlush, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

    if width == 0 || height == 0 {
        return None;
    }
    let pixel_count = (width as usize).checked_mul(height as usize)?;
    let byte_len = pixel_count.checked_mul(4)?;
    if rgba.len() < byte_len {
        return None;
    }

    unsafe {
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width as i32;
        // Hauteur négative : lignes stockées de haut en bas, comme l'image source.
        info.bmiHeader.biHeight = -(height as i32);
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;

        let mut bits: *mut c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(color.0));
            return None;
        }
        let destination = std::slice::from_raw_parts_mut(bits as *mut u8, byte_len);
        for (source, target) in rgba[..byte_len]
            .chunks_exact(4)
            .zip(destination.chunks_exact_mut(4))
        {
            target[0] = source[2];
            target[1] = source[1];
            target[2] = source[0];
            target[3] = source[3];
        }
        let _ = GdiFlush();

        let mask_stride = (width as usize).div_ceil(16) * 2;
        let mask_bits = vec![0u8; mask_stride * height as usize];
        let mask = CreateBitmap(
            width as i32,
            height as i32,
            1,
            1,
            Some(mask_bits.as_ptr().cast()),
        );
        if mask.0.is_null() {
            let _ = DeleteObject(HGDIOBJ(color.0));
            return None;
        }

        let icon_info = ICONINFO {
            fIcon: TRUE,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon: Option<HICON> = CreateIconIndirect(&icon_info).ok();
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        icon
    }
}

/// Remplit `szTip` (127 unités UTF-16 utiles + NUL final) sans couper une paire de substitution.
#[cfg(windows)]
fn tooltip_field(tip: &str) -> [u16; 128] {
    let mut buffer = [0u16; 128];
    let mut length = 0usize;
    for character in tip.chars() {
        let mut encoded = [0u16; 2];
        let units = character.encode_utf16(&mut encoded);
        if length + units.len() > 127 {
            break;
        }
        for unit in units {
            buffer[length] = *unit;
            length += 1;
        }
    }
    buffer
}

#[cfg(windows)]
unsafe extern "system" fn tray_wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, DestroyWindow, PostQuitMessage, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY,
        WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP,
    };

    match msg {
        TRAY_CALLBACK_MESSAGE => {
            match (lparam.0 as u32) & 0xFFFF {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => send_command(TrayCommand::ShowWindow),
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    unsafe { show_tray_menu(hwnd) };
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Affiche le menu contextuel natif et dispatche l'entrée choisie.
#[cfg(windows)]
unsafe fn show_tray_menu(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, MF_SEPARATOR, MF_STRING,
        PostMessageW, SetForegroundWindow, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
        TrackPopupMenu, WM_NULL,
    };
    use windows::core::{PCWSTR, w};

    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(menu, MF_STRING, MENU_SHOW as usize, w!("Afficher TideNative"));
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            MENU_TOGGLE as usize,
            w!("Lecture / Pause"),
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            MENU_PREVIOUS as usize,
            w!("Titre précédent"),
        );
        let _ = AppendMenuW(menu, MF_STRING, MENU_NEXT as usize, w!("Titre suivant"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT as usize, w!("Quitter"));

        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        // Ouvre le menu au premier plan : sans cela il reste affiché après le clic.
        let _ = SetForegroundWindow(hwnd);
        let selected = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));

        match selected.0 as u32 {
            MENU_SHOW => send_command(TrayCommand::ShowWindow),
            MENU_TOGGLE => send_command(TrayCommand::TogglePlayPause),
            MENU_PREVIOUS => send_command(TrayCommand::Previous),
            MENU_NEXT => send_command(TrayCommand::Next),
            MENU_QUIT => send_command(TrayCommand::Quit),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Aides fenêtre (Windows)
// ---------------------------------------------------------------------------

/// Restaure et met au premier plan la fenêtre principale (`hwnd == 0` ignoré).
#[cfg(windows)]
pub fn show_and_focus_window(hwnd: u64) {
    use std::ffi::c_void;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SW_RESTORE, SetForegroundWindow, ShowWindow};

    if hwnd == 0 {
        return;
    }
    let handle = HWND(hwnd as usize as *mut c_void);
    unsafe {
        let _ = ShowWindow(handle, SW_RESTORE);
        let _ = SetForegroundWindow(handle);
    }
}

/// Masque la fenêtre principale vers la zone de notification (`hwnd == 0` ignoré).
#[cfg(windows)]
pub fn hide_window_to_tray(hwnd: u64) {
    use std::ffi::c_void;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    if hwnd == 0 {
        return;
    }
    unsafe {
        let _ = ShowWindow(HWND(hwnd as usize as *mut c_void), SW_HIDE);
    }
}

// ---------------------------------------------------------------------------
// Stubs non Windows
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
pub struct TrayController;

#[cfg(not(windows))]
impl TrayController {
    pub fn spawn(
        _icon_rgba: Vec<u8>,
        _icon_w: u32,
        _icon_h: u32,
        _cmd_tx: UnboundedSender<TrayCommand>,
    ) -> Result<Self> {
        Ok(Self)
    }

    pub fn update_tooltip(&self, _tip: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn show_and_focus_window(_hwnd: u64) {}

#[cfg(not(windows))]
pub fn hide_window_to_tray(_hwnd: u64) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_hwnd_helpers_are_safe_no_ops() {
        show_and_focus_window(0);
        hide_window_to_tray(0);
    }

    #[test]
    fn spawn_update_tooltip_and_drop_complete_cleanly() {
        let mut icon = Vec::with_capacity(16 * 16 * 4);
        for y in 0..16u8 {
            for x in 0..16u8 {
                icon.extend_from_slice(&[x * 16, y * 16, 128, 255]);
            }
        }

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let controller = TrayController::spawn(icon, 16, 16, tx).expect("icône de notification");
        // Une session sans explorateur peut refuser NIM_MODIFY : seul le
        // déroulement sans blocage est garanti ici.
        let _ = controller.update_tooltip("TideNative — écoute en cours");
        let _ = controller.update_tooltip(&"é".repeat(200));
        assert!(rx.try_recv().is_err(), "aucune commande sans interaction");

        drop(controller);
        assert!(rx.try_recv().is_err());
    }

    #[cfg(windows)]
    #[test]
    fn tooltip_is_truncated_to_127_utf16_units_without_splitting_surrogates() {
        // 130 caractères accentés : 127 unités maximum, NUL final conservé.
        let truncated = tooltip_field(&"é".repeat(130));
        let units: Vec<u16> = truncated.iter().copied().take_while(|u| *u != 0).collect();
        assert_eq!(units.len(), 127);
        assert_eq!(truncated[127], 0);

        // Un caractère hors BMP (paire de substitution) au bord de la limite
        // est écarté en entier plutôt que coupé au milieu.
        let mut tip = "a".repeat(126);
        tip.push('\u{1D11E}');
        tip.push('b');
        let units: Vec<u16> = tooltip_field(&tip)
            .iter()
            .copied()
            .take_while(|u| *u != 0)
            .collect();
        assert_eq!(units.len(), 126);
        assert!(units.iter().all(|unit| *unit == u16::from(b'a')));
    }
}
