//! Lectura de cookies del perfil WebView2 (sesión de claude.ai).
//! La app nunca ve la contraseña: solo reutiliza la cookie que el propio
//! webview persiste tras el login manual del usuario (SPEC §3).

#[cfg(windows)]
pub fn get_cookie_header(app: &tauri::AppHandle) -> Option<String> {
    use std::sync::mpsc;
    use std::time::Duration;
    use tauri::Manager;

    // el gestor de cookies es del perfil completo: sirve cualquier webview vivo
    let win = app.get_webview_window("bar")?;
    let (tx, rx) = mpsc::channel::<String>();

    let res = win.with_webview(move |webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2CookieList, ICoreWebView2_2,
        };
        use webview2_com::GetCookiesCompletedHandler;
        use windows::core::{Interface, HSTRING, PWSTR};
        unsafe {
            let controller = webview.controller();
            let Ok(core) = controller.CoreWebView2() else { return };
            let Ok(core2) = core.cast::<ICoreWebView2_2>() else { return };
            let Ok(mgr) = core2.CookieManager() else { return };
            let handler = GetCookiesCompletedHandler::create(Box::new(
                move |_hr, list: Option<ICoreWebView2CookieList>| {
                    let mut out = String::new();
                    if let Some(list) = list {
                        let mut n: u32 = 0;
                        let _ = list.Count(&mut n);
                        for i in 0..n {
                            let Ok(c) = list.GetValueAtIndex(i) else {
                                continue;
                            };
                            let mut name = PWSTR::null();
                            let mut val = PWSTR::null();
                            if c.Name(&mut name).is_ok() && c.Value(&mut val).is_ok() {
                                let name = webview2_com::take_pwstr(name);
                                let val = webview2_com::take_pwstr(val);
                                if !name.is_empty() {
                                    out.push_str(&format!("{}={}; ", name, val));
                                }
                            }
                        }
                    }
                    let _ = tx.send(out);
                    Ok(())
                },
            ));
            let _ = mgr.GetCookies(&HSTRING::from("https://claude.ai"), &handler);
        }
    });
    if res.is_err() {
        return None;
    }
    rx.recv_timeout(Duration::from_secs(5))
        .ok()
        .map(|s| s.trim_end_matches([' ', ';']).to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(windows))]
pub fn get_cookie_header(_app: &tauri::AppHandle) -> Option<String> {
    None
}
