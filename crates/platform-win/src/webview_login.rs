//! Fenêtre navigateur WebView2 éphémère **dédiée à la connexion sur le site officiel TIDAL**
//! (`https://listen.tidal.com`), exactement comme si l'utilisateur ouvrait TIDAL dans son navigateur :
//! - Aucun Client ID, Client Secret ni fichier JSON à configurer.
//! - Chargement dynamique de `EmbeddedBrowserWebView.dll` (zéro DLL externe, zéro RAM WebView2 hors login).
//! - Capture automatique de la session authentifiée dès que l'utilisateur se connecte sur TIDAL :
//!   1. Réponse OAuth `https://auth.tidal.com/v1/oauth2/token` (`access_token`, `refresh_token`, `expires_in`)
//!   2. Et en complément, en-tête `Authorization: Bearer <JWT>` des appels API TIDAL (`api.tidal.com` / `listen.tidal.com`).
//! - Fermeture et destruction immédiates de la fenêtre WebView2 dès la capture de la session.

#[cfg(not(windows))]
use crate::PlatformError;
use crate::Result;
use std::path::PathBuf;

/// Session TIDAL capturée directement depuis la page web officielle `listen.tidal.com`.
#[derive(Debug, Clone)]
pub struct WebTidalSession {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
}

/// Résultat d'une fenêtre de connexion web TIDAL.
#[derive(Debug, Clone)]
pub enum WebViewLoginOutcome {
    /// Session utilisateur authentifiée capturée sur le site TIDAL.
    SessionCaptured(WebTidalSession),
    /// L'utilisateur a fermé la fenêtre avant la fin de la connexion.
    Cancelled,
}

/// Ouvre une fenêtre navigateur WebView2 sombre directement sur `https://listen.tidal.com`
/// afin que l'utilisateur se connecte à son compte TIDAL normalement.
///
/// Fonction bloquante à appeler depuis `tokio::task::spawn_blocking`.
#[cfg(windows)]
pub fn run_tidal_web_login(user_data_dir: PathBuf) -> Result<WebViewLoginOutcome> {
    win_impl::run("https://listen.tidal.com".to_string(), user_data_dir)
}

#[cfg(not(windows))]
pub fn run_tidal_web_login(_user_data_dir: PathBuf) -> Result<WebViewLoginOutcome> {
    Err(PlatformError::WebView(
        "WebView2 disponible uniquement sur Windows".into(),
    ))
}

#[cfg(windows)]
mod win_impl {
    use super::{WebTidalSession, WebViewLoginOutcome};
    use crate::dwm::apply_windows11_styling;
    use crate::{PlatformError, Result};
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, ICoreWebView2, ICoreWebView2_2,
        ICoreWebView2Controller, ICoreWebView2Environment,
    };
    use webview2_com::{
        CreateCoreWebView2ControllerCompletedHandler,
        CreateCoreWebView2EnvironmentCompletedHandler, WebResourceRequestedEventHandler,
        WebResourceResponseReceivedEventHandler,
        WebResourceResponseViewGetContentCompletedHandler, take_pwstr,
    };
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::CreateSolidBrush;
    use windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize, IStream,
    };
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
        GetClientRect, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, IDC_ARROW, LoadCursorW,
        MSG, PostMessageW, PostQuitMessage, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
        SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage, WM_CLOSE,
        WM_DESTROY, WM_SIZE, WNDCLASSW, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };
    use windows::core::{HRESULT, HSTRING, Interface, PCSTR, PCWSTR, PWSTR, w};

    /// DLL officielle Microsoft `WebView2Loader.dll` (architecture correspondante) embarquée dans l'exécutable :
    /// - Zéro DLL externe à distribuer avec `TideNative.exe`
    /// - Zéro dépendance d'import PE sur `WebView2Loader.dll` (immunité totale contre les DLL 32 bits du PATH / 0xc000007b)
    static EMBEDDED_WEBVIEW2_LOADER: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/WebView2Loader.dll"));

    type CreateEnvWithOptionsFn = unsafe extern "system" fn(
        browser_executable_folder: PCWSTR,
        user_data_folder: PCWSTR,
        environment_options: *mut c_void,
        environment_created_handler: *mut c_void,
    ) -> HRESULT;

    fn load_embedded_webview2_create_env() -> Result<CreateEnvWithOptionsFn> {
        let dir = tidal_core::paths::app_data_dir().map_err(|e| {
            PlatformError::WebView(format!("dossier TideNative indisponible : {e}"))
        })?;
        #[cfg(target_arch = "x86_64")]
        let dll_name = "WebView2Loader-x64.dll";
        #[cfg(target_arch = "x86")]
        let dll_name = "WebView2Loader-x86.dll";
        #[cfg(target_arch = "aarch64")]
        let dll_name = "WebView2Loader-arm64.dll";

        let dll_path = tidal_core::paths::normalize_path(&dir.join(dll_name));
        let needs_write = match std::fs::metadata(&dll_path) {
            Ok(meta) => meta.len() != EMBEDDED_WEBVIEW2_LOADER.len() as u64,
            Err(_) => true,
        };
        if needs_write {
            let tmp = dll_path.with_extension("dll.tmp");
            if std::fs::write(&tmp, EMBEDDED_WEBVIEW2_LOADER).is_ok() {
                if std::fs::rename(&tmp, &dll_path).is_err() {
                    let _ = std::fs::remove_file(&tmp);
                    let _ = std::fs::write(&dll_path, EMBEDDED_WEBVIEW2_LOADER);
                }
            }
        }

        unsafe {
            let path_h = HSTRING::from(dll_path.as_os_str());
            let hmod = LoadLibraryW(PCWSTR(path_h.as_ptr())).map_err(|e| {
                PlatformError::WebView(format!(
                    "Impossible de charger {} : {e}",
                    dll_path.display()
                ))
            })?;
            let proc_addr = GetProcAddress(
                hmod,
                PCSTR(c"CreateCoreWebView2EnvironmentWithOptions".as_ptr().cast()),
            )
            .ok_or_else(|| {
                PlatformError::WebView(
                    "Symbole CreateCoreWebView2EnvironmentWithOptions introuvable".into(),
                )
            })?;
            Ok(std::mem::transmute::<_, CreateEnvWithOptionsFn>(proc_addr))
        }
    }

    struct WindowState {
        controller: RefCell<Option<ICoreWebView2Controller>>,
        webview: RefCell<Option<ICoreWebView2>>,
    }

    pub fn run(start_url: String, user_data_dir: PathBuf) -> Result<WebViewLoginOutcome> {
        std::fs::create_dir_all(&user_data_dir).map_err(|e| {
            PlatformError::WebView(format!(
                "impossible de créer le profil WebView2 {} : {e}",
                user_data_dir.display()
            ))
        })?;
        let user_data_dir = tidal_core::paths::normalize_path(&user_data_dir);

        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() {
                return Err(PlatformError::WebView(format!(
                    "CoInitializeEx STA échoué : {hr:?}"
                )));
            }
        }

        struct ComGuard;
        impl Drop for ComGuard {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }
        let _com_guard = ComGuard;

        let captured = Arc::new(Mutex::new(None::<WebTidalSession>));
        let init_error = Arc::new(Mutex::new(None::<String>));
        let state = Box::new(WindowState {
            controller: RefCell::new(None),
            webview: RefCell::new(None),
        });
        let state_ptr = Box::into_raw(state);

        let hwnd = create_login_window(state_ptr)?;
        let _ = apply_windows11_styling(hwnd.0 as isize, true);

        let hwnd_raw = hwnd.0 as isize;
        let state_ptr_addr = state_ptr as usize;
        let url_clone = start_url.clone();
        let captured_clone = captured.clone();
        let init_err_clone = init_error.clone();

        let env_handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |hr, env_opt| {
                if let Err(e) = hr {
                    if let Ok(mut g) = init_err_clone.lock() {
                        *g = Some(format!("Initialisation WebView2 échouée : {e}"));
                    }
                    post_close(hwnd_raw);
                    return Ok(());
                }
                let Some(env): Option<ICoreWebView2Environment> = env_opt else {
                    if let Ok(mut g) = init_err_clone.lock() {
                        *g = Some("ICoreWebView2Environment nul".into());
                    }
                    post_close(hwnd_raw);
                    return Ok(());
                };

                let url_inner = url_clone.clone();
                let captured_inner = captured_clone.clone();
                let init_err_inner = init_err_clone.clone();

                let ctrl_handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                    move |hr, ctrl_opt| {
                        if let Err(e) = hr {
                            if let Ok(mut g) = init_err_inner.lock() {
                                *g = Some(format!("Contrôleur WebView2 échoué : {e}"));
                            }
                            post_close(hwnd_raw);
                            return Ok(());
                        }
                        let Some(controller) = ctrl_opt else {
                            return Ok(());
                        };

                        unsafe {
                            let hwnd_local = HWND(hwnd_raw as *mut c_void);
                            let mut rect = RECT::default();
                            let _ = GetClientRect(hwnd_local, &mut rect);
                            let _ = controller.SetBounds(rect);
                            let _ = controller.SetIsVisible(true);

                            if let Ok(webview) = controller.CoreWebView2() {
                                // 1. Intercepte les réponses OAuth `auth.tidal.com/v1/oauth2/token`
                                if let Ok(wv2) = webview.cast::<ICoreWebView2_2>() {
                                    let cap_resp = captured_inner.clone();
                                    let mut resp_token: i64 = 0;
                                    let resp_handler =
                                        WebResourceResponseReceivedEventHandler::create(Box::new(
                                            move |_wv, args_opt| {
                                                let Some(args) = args_opt else {
                                                    return Ok(());
                                                };
                                                let Ok(req) = args.Request() else {
                                                    return Ok(());
                                                };
                                                let mut uri_raw = PWSTR::null();
                                                if req.Uri(&mut uri_raw).is_err() {
                                                    return Ok(());
                                                }
                                                let uri = take_pwstr(uri_raw);
                                                if !uri.contains("auth.tidal.com/v1/oauth2/token") {
                                                    return Ok(());
                                                }
                                                let Ok(resp) = args.Response() else {
                                                    return Ok(());
                                                };
                                                let mut status: i32 = 0;
                                                if resp.StatusCode(&mut status).is_err()
                                                    || !(200..300).contains(&status)
                                                {
                                                    return Ok(());
                                                }

                                                let cap_content = cap_resp.clone();
                                                let content_handler =
                                                    WebResourceResponseViewGetContentCompletedHandler::create(
                                                        Box::new(move |hr_c, stream_opt| {
                                                            if hr_c.is_ok() {
                                                                if let Some(stream) = stream_opt {
                                                                    if let Some(body) =
                                                                        read_istream_utf8(&stream)
                                                                    {
                                                                        if let Some(sess) =
                                                                            parse_oauth_token_json(
                                                                                &body,
                                                                            )
                                                                        {
                                                                            store_and_close(
                                                                                &cap_content,
                                                                                sess,
                                                                                hwnd_raw,
                                                                            );
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            Ok(())
                                                        }),
                                                    );
                                                let _ = resp.GetContent(&content_handler);
                                                Ok(())
                                            },
                                        ));
                                    let _ = wv2.add_WebResourceResponseReceived(
                                        &resp_handler,
                                        &mut resp_token,
                                    );
                                }

                                // 2. Filet de sécurité : capture `Authorization: Bearer <JWT>`
                                //    sur les appels API TIDAL (fonctionne aussi si déjà connecté via cookies)
                                let _ = webview.AddWebResourceRequestedFilter(
                                    w!("*tidal.com*"),
                                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                                );
                                let cap_req = captured_inner.clone();
                                let mut req_token: i64 = 0;
                                let req_handler = WebResourceRequestedEventHandler::create(
                                    Box::new(move |_wv, args_opt| {
                                        let Some(args) = args_opt else {
                                            return Ok(());
                                        };
                                        let Ok(req) = args.Request() else {
                                            return Ok(());
                                        };
                                        let mut uri_raw = PWSTR::null();
                                        if req.Uri(&mut uri_raw).is_err() {
                                            return Ok(());
                                        }
                                        let uri = take_pwstr(uri_raw);
                                        let is_tidal_api = uri.contains("api.tidal.com/v")
                                            || uri.contains("listen.tidal.com/v1/")
                                            || uri.contains("listen.tidal.com/v2/")
                                            || uri.contains("openapi.tidal.com/v");
                                        if !is_tidal_api {
                                            return Ok(());
                                        }
                                        let Ok(headers) = req.Headers() else {
                                            return Ok(());
                                        };
                                        let mut auth_raw = PWSTR::null();
                                        if headers
                                            .GetHeader(w!("Authorization"), &mut auth_raw)
                                            .is_ok()
                                        {
                                            let auth_val = take_pwstr(auth_raw);
                                            if let Some(jwt) = extract_user_bearer_jwt(&auth_val) {
                                                let sess = WebTidalSession {
                                                    access_token: jwt,
                                                    refresh_token: None,
                                                    expires_in: None,
                                                };
                                                store_and_close(&cap_req, sess, hwnd_raw);
                                            }
                                        }
                                        Ok(())
                                    }),
                                );
                                let _ = webview.add_WebResourceRequested(&req_handler, &mut req_token);

                                // Ouvre directement le vrai site web TIDAL
                                let target = HSTRING::from(url_inner.as_str());
                                let _ = webview.Navigate(PCWSTR(target.as_ptr()));

                                let st = &*(state_ptr_addr as *const WindowState);
                                *st.webview.borrow_mut() = Some(webview);
                            }

                            let st = &*(state_ptr_addr as *const WindowState);
                            *st.controller.borrow_mut() = Some(controller);
                        }
                        Ok(())
                    },
                ));

                unsafe {
                    let _ = env.CreateCoreWebView2Controller(
                        HWND(hwnd_raw as *mut c_void),
                        &ctrl_handler,
                    );
                }
                Ok(())
            },
        ));

        let create_env = load_embedded_webview2_create_env()?;
        let user_data_h = HSTRING::from(user_data_dir.as_os_str());
        let create_res = unsafe {
            create_env(
                PCWSTR::null(),
                PCWSTR(user_data_h.as_ptr()),
                std::ptr::null_mut(),
                env_handler.as_raw(),
            )
            .ok()
        };

        if let Err(e) = create_res {
            unsafe {
                let _ = DestroyWindow(hwnd);
                let _ = Box::from_raw(state_ptr);
            }
            return Err(PlatformError::WebView(format!(
                "Impossible de démarrer WebView2 ({e}). Vérifiez que Microsoft Edge WebView2 Runtime est installé."
            )));
        }

        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let st = Box::from_raw(state_ptr);
            if let Some(ctrl) = st.controller.borrow_mut().take() {
                let _ = ctrl.Close();
            }
            let _ = st.webview.borrow_mut().take();
        }

        if let Ok(mut g) = init_error.lock() {
            if let Some(err) = g.take() {
                return Err(PlatformError::WebView(err));
            }
        }
        if let Ok(mut g) = captured.lock() {
            if let Some(sess) = g.take() {
                return Ok(WebViewLoginOutcome::SessionCaptured(sess));
            }
        }
        Ok(WebViewLoginOutcome::Cancelled)
    }

    fn post_close(hwnd_raw: isize) {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_raw as *mut c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }

    fn store_and_close(
        slot: &Arc<Mutex<Option<WebTidalSession>>>,
        sess: WebTidalSession,
        hwnd_raw: isize,
    ) {
        if let Ok(mut g) = slot.lock() {
            // Priorise une session complète (avec refresh_token) si disponible
            let should_store = match g.as_ref() {
                None => true,
                Some(existing) => existing.refresh_token.is_none() && sess.refresh_token.is_some(),
            };
            if should_store {
                *g = Some(sess);
            }
        }
        post_close(hwnd_raw);
    }

    fn read_istream_utf8(stream: &IStream) -> Option<String> {
        let mut out = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let mut read: u32 = 0;
            let hr = unsafe {
                stream.Read(
                    chunk.as_mut_ptr() as *mut c_void,
                    chunk.len() as u32,
                    Some(&mut read),
                )
            };
            if hr.is_err() || read == 0 {
                break;
            }
            out.extend_from_slice(&chunk[..read as usize]);
            if out.len() > 256 * 1024 {
                break;
            }
        }
        String::from_utf8(out).ok()
    }

    fn parse_oauth_token_json(body: &str) -> Option<WebTidalSession> {
        let v: serde_json::Value = serde_json::from_str(body).ok()?;
        let access_token = v.get("access_token")?.as_str()?.trim().to_string();
        if access_token.len() < 80 {
            return None;
        }
        let refresh_token = v
            .get("refresh_token")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let has_user_field = v.get("user_id").is_some()
            || v.get("user").and_then(|u| u.get("userId")).is_some();
        if refresh_token.is_none() && !has_user_field && !jwt_has_user_claim(&access_token) {
            // Jeton anonyme / client_credentials : attendre la vraie connexion utilisateur.
            return None;
        }
        let expires_in = v.get("expires_in").and_then(|x| x.as_u64());
        Some(WebTidalSession {
            access_token,
            refresh_token,
            expires_in,
        })
    }

    fn extract_user_bearer_jwt(header_value: &str) -> Option<String> {
        let token = header_value
            .trim()
            .strip_prefix("Bearer ")
            .or_else(|| header_value.trim().strip_prefix("bearer "))?
            .trim();
        // Les jetons utilisateur TIDAL sont des JWT (3 segments séparés par '.', > 100 car.)
        // et contiennent un champ `uid` (utilisateur authentifié, ≠ jeton invité).
        if token.len() > 100
            && token.starts_with("eyJ")
            && token.matches('.').count() == 2
            && jwt_has_user_claim(token)
        {
            Some(token.to_string())
        } else {
            None
        }
    }

    /// Vérifie sans dépendance externe que le payload base64url d'un JWT TIDAL
    /// contient une identité utilisateur (`uid` non nul).
    fn jwt_has_user_claim(jwt: &str) -> bool {
        let mut parts = jwt.split('.');
        let _header = match parts.next() {
            Some(h) => h,
            None => return false,
        };
        let payload_b64 = match parts.next() {
            Some(p) => p,
            None => return false,
        };
        let bytes = match decode_base64url(payload_b64) {
            Some(b) => b,
            None => return false,
        };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return false;
        };
        if let Some(uid) = v.get("uid") {
            if let Some(n) = uid.as_u64() {
                return n > 0;
            }
            if let Some(s) = uid.as_str() {
                return !s.is_empty() && s != "0";
            }
        }
        false
    }

    fn decode_base64url(input: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(input.len() * 3 / 4);
        let mut buf: u32 = 0;
        let mut bits: u8 = 0;
        for b in input.bytes() {
            let val = match b {
                b'A'..=b'Z' => b - b'A',
                b'a'..=b'z' => b - b'a' + 26,
                b'0'..=b'9' => b - b'0' + 52,
                b'-' | b'+' => 62,
                b'_' | b'/' => 63,
                b'=' => break,
                _ => return None,
            };
            buf = (buf << 6) | u32::from(val);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
                buf &= (1 << bits) - 1;
            }
        }
        Some(out)
    }

    fn create_login_window(state_ptr: *mut WindowState) -> Result<HWND> {
        unsafe {
            let hinstance = GetModuleHandleW(None)
                .map_err(|e| PlatformError::WebView(format!("GetModuleHandleW : {e}")))?;
            let class_name = w!("TideNativeTidalWebLogin");

            let bg_brush = CreateSolidBrush(COLORREF(0x00090707));
            let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();

            let wc = WNDCLASSW {
                lpfnWndProc: Some(login_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hbrBackground: bg_brush,
                hCursor: cursor,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            let width = 1080;
            let height = 760;
            let sx = GetSystemMetrics(SM_CXSCREEN);
            let sy = GetSystemMetrics(SM_CYSCREEN);
            let x = ((sx - width) / 2).max(40);
            let y = ((sy - height) / 2).max(40);

            let hwnd = CreateWindowExW(
                WS_EX_APPWINDOW,
                class_name,
                w!("Connexion à TIDAL — TideNative"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
            .map_err(|e| PlatformError::WebView(format!("CreateWindowExW : {e}")))?;

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            Ok(hwnd)
        }
    }

    unsafe extern "system" fn login_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_SIZE => {
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WindowState;
                if !ptr.is_null() {
                    let st = unsafe { &*ptr };
                    if let Some(ctrl) = st.controller.borrow().as_ref() {
                        let mut rect = RECT::default();
                        unsafe {
                            let _ = GetClientRect(hwnd, &mut rect);
                            let _ = ctrl.SetBounds(rect);
                        }
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe {
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn creates_webview2_environment_with_static_loader() {
            let tmp = std::env::temp_dir().join("tidenative-wv2-env-test");
            let _ = std::fs::create_dir_all(&tmp);
            let tmp = tidal_core::paths::normalize_path(&tmp);

            let result = Arc::new(Mutex::new(None::<std::result::Result<(), String>>));
            let res_clone = result.clone();

            unsafe {
                let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                assert!(hr.is_ok(), "CoInitializeEx failed: {hr:?}");

                let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                    move |hr_env, env_opt| {
                        let outcome = match (hr_env, env_opt) {
                            (Ok(()), Some(_env)) => Ok(()),
                            (Err(e), _) => Err(format!("env hr error: {e}")),
                            (Ok(()), None) => Err("env was None".into()),
                        };
                        if let Ok(mut g) = res_clone.lock() {
                            *g = Some(outcome);
                        }
                        PostQuitMessage(0);
                        Ok(())
                    },
                ));

                let create_env = load_embedded_webview2_create_env()
                    .expect("load_embedded_webview2_create_env failed");
                let dir_h = HSTRING::from(tmp.as_os_str());
                create_env(
                    PCWSTR::null(),
                    PCWSTR(dir_h.as_ptr()),
                    std::ptr::null_mut(),
                    handler.as_raw(),
                )
                .ok()
                .expect("CreateCoreWebView2EnvironmentWithOptions call failed");

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                CoUninitialize();
            }

            let _ = std::fs::remove_dir_all(&tmp);
            let outcome = result
                .lock()
                .unwrap()
                .take()
                .expect("WebView2 environment callback did not fire");
            assert!(outcome.is_ok(), "WebView2 environment creation failed: {outcome:?}");
        }

        #[test]
        fn extracts_valid_tidal_jwt_bearer() {
            // Payload base64url de {"uid":123456789,"cid":"tidal-web-client-placeholder"}
            let header = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9";
            let user_payload = "eyJ1aWQiOjEyMzQ1Njc4OSwiY2lkIjoidGlkYWwtd2ViLWNsaWVudC1wbGFjZWhvbGRlciJ9";
            let guest_payload = "eyJjaWQiOiJ0aWRhbC13ZWItY2xpZW50LXBsYWNlaG9sZGVyIn0";
            let sig = "a".repeat(64);
            let user_jwt = format!("Bearer {header}.{user_payload}.{sig}");
            let guest_jwt = format!("Bearer {header}.{guest_payload}.{sig}");
            assert!(extract_user_bearer_jwt(&user_jwt).is_some());
            assert!(extract_user_bearer_jwt(&guest_jwt).is_none());
            assert!(extract_user_bearer_jwt("Bearer short").is_none());
            assert!(extract_user_bearer_jwt("Basic abc").is_none());
        }
    }
}
