// ClaudeClock — barra overlay de consumo de la suscripción Claude (SPEC-claudeclock.md)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod barpos;
mod config;
mod cookies;
mod history;
mod projection;
mod usage;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use config::Config;
use history::{History, Point};
use usage::{DemoSource, UsagePayload, SSTEP, WSTEP};

pub struct AppState {
    pub demo: bool,
    /// arranque con Windows: solo el overlay, sin abrir dashboard ni login
    autostart: bool,
    pub cfg: Mutex<Config>,
    cfg_path: PathBuf,
    usage: Mutex<Option<UsagePayload>>,
    history: Mutex<History>,
    demo_src: Mutex<DemoSource>,
    paused: AtomicBool,
    bar_mon: Mutex<barpos::Rect>,
    /// ancho medido del contenido de la barra (modos Compacto)
    bar_w: Mutex<Option<u32>>,
    /// uuid de la organización de claude.ai (descubierto una vez)
    pub org: Mutex<Option<String>>,
    /// access token renovado en memoria (token, expira_ms) — nunca se persiste
    pub oauth: Mutex<Option<(String, i64)>>,
    /// fuente activa de datos: "token" (Claude Code) | "cookie" (claude.ai) | ""
    source: Mutex<String>,
    /// hay sesión válida y el endpoint responde
    connected: AtomicBool,
    /// despierta el poll loop de inmediato (p. ej. al cerrar el login)
    notify: tokio::sync::Notify,
}

/// Emite el estado de conexión a todas las ventanas cuando cambia.
fn set_connected(app: &AppHandle, ok: bool, source: &str) {
    let state = app.state::<AppState>();
    let src_changed = {
        let mut s = state.source.lock().unwrap();
        let changed = *s != source;
        *s = source.to_string();
        changed
    };
    if state.connected.swap(ok, Ordering::Relaxed) != ok || src_changed {
        let _ = app.emit(
            "status",
            serde_json::json!({ "connected": ok, "demo": state.demo, "source": source }),
        );
    }
}

/// Registra/elimina la app en el arranque de Windows (HKCU\...\Run).
/// Con --autostart la app inicia solo con el overlay visible.
#[cfg(windows)]
fn apply_autostart(enable: bool) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Ok(exe) = std::env::current_exe() else { return };
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut cmd = std::process::Command::new("reg");
    if enable {
        cmd.args([
            "add",
            key,
            "/v",
            "ClaudeClock",
            "/t",
            "REG_SZ",
            "/d",
            &format!("\"{}\" --autostart", exe.display()),
            "/f",
        ]);
    } else {
        cmd.args(["delete", key, "/v", "ClaudeClock", "/f"]);
    }
    let _ = cmd.creation_flags(CREATE_NO_WINDOW).output();
}
#[cfg(not(windows))]
fn apply_autostart(_enable: bool) {}

/// Reposiciona la barra con el ancho compacto vigente y guarda el rect del monitor.
fn reposition(app: &AppHandle, state: &AppState, cfg: &Config) {
    if let Some(bar) = app.get_webview_window("bar") {
        let bw = *state.bar_w.lock().unwrap();
        if let Some(rect) = barpos::position_bar(&bar, cfg, bw) {
            *state.bar_mon.lock().unwrap() = rect;
        }
    }
}

fn now_ts() -> i64 {
    chrono::Local::now().timestamp()
}

/* ============================ comandos ============================ */

#[derive(Serialize)]
struct BootInfo {
    config: Config,
    demo: bool,
    connected: bool,
    source: String,
    usage: Option<UsagePayload>,
    monitors: Vec<String>,
    paused: bool,
}

#[tauri::command]
fn boot(app: AppHandle, state: State<AppState>) -> BootInfo {
    let monitors = app
        .get_webview_window("bar")
        .and_then(|w| w.available_monitors().ok())
        .map(|ms| {
            ms.iter()
                .enumerate()
                .map(|(i, m)| m.name().cloned().unwrap_or_else(|| format!("Monitor {}", i + 1)))
                .collect()
        })
        .unwrap_or_default();
    BootInfo {
        config: state.cfg.lock().unwrap().clone(),
        demo: state.demo,
        connected: state.demo || state.connected.load(Ordering::Relaxed),
        source: state.source.lock().unwrap().clone(),
        usage: state.usage.lock().unwrap().clone(),
        monitors,
        paused: state.paused.load(Ordering::Relaxed),
    }
}

#[tauri::command]
fn set_config(app: AppHandle, state: State<AppState>, cfg: Config) {
    let (reposition, click_through, autostart_changed) = {
        let mut cur = state.cfg.lock().unwrap();
        let repos = cur.monitor != cfg.monitor
            || cur.alto_modo != cfg.alto_modo
            || cur.posicion != cfg.posicion;
        let ct = cur.click_through != cfg.click_through;
        let auto = cur.autoarranque != cfg.autoarranque;
        *cur = cfg.clone();
        (repos, ct, auto)
    };
    config::save(&state.cfg_path, &cfg);
    if reposition {
        self::reposition(&app, &state, &cfg);
    }
    if click_through {
        if let Some(bar) = app.get_webview_window("bar") {
            let _ = bar.set_ignore_cursor_events(cfg.click_through);
        }
    }
    if autostart_changed {
        apply_autostart(cfg.autoarranque);
    }
    refresh_tray(&app);
    let _ = app.emit("config-changed", &cfg);
}

/// La barra midió su contenido (modo Compacto) y pide encogerse a ese ancho.
#[tauri::command]
fn resize_bar(app: AppHandle, state: State<AppState>, width: u32) {
    *state.bar_w.lock().unwrap() = Some(width);
    let cfg = state.cfg.lock().unwrap().clone();
    if cfg.alto_modo.starts_with("compacto") {
        reposition(&app, &state, &cfg);
    }
}

#[derive(Serialize)]
struct GraphOut {
    buckets: Vec<f64>,
    start_ts: i64,
    step_secs: i64,
}

#[tauri::command]
fn get_graph(state: State<AppState>, meter: String) -> GraphOut {
    let now = now_ts();
    let (start, step, n) = if meter == "s" {
        let reset_s = state
            .usage
            .lock()
            .unwrap()
            .as_ref()
            .map(|u| u.reset_s)
            .unwrap_or(now + 5 * 3600);
        (reset_s - 5 * 3600, SSTEP, 10) // ventana móvil de 5 h, 10 barras de 30 min
    } else {
        ((now / WSTEP) * WSTEP - 27 * WSTEP, WSTEP, 28) // 7 días, 28 barras de 6 h
    };
    let buckets = state.history.lock().unwrap().buckets(&meter, start, step, n);
    GraphOut { buckets, start_ts: start, step_secs: step }
}

#[tauri::command]
fn get_projection(state: State<AppState>, meter: String) -> Option<String> {
    let now = now_ts();
    let current = {
        let u = state.usage.lock().unwrap();
        let u = u.as_ref()?;
        match meter.as_str() {
            "s" => u.s,
            "w" => u.w,
            _ => u.f,
        }
    };
    let hours = if meter == "s" { 1.0 } else { 24.0 };
    let rate = state.history.lock().unwrap().rate_per_hour(&meter, now, hours);
    projection::project(current, rate, now)
}

/// Las ventanas se crean UNA sola vez al arranque (crear webviews en runtime
/// deja la ventana en blanco en algunos equipos); abrir = mostrar + foco.
fn open_window(app: &AppHandle, label: &str) {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn open_main(app: AppHandle) {
    open_window(&app, "main");
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    open_window(&app, "settings");
}

#[tauri::command]
fn clear_history(state: State<AppState>) {
    state.history.lock().unwrap().clear();
}

#[tauri::command]
fn relogin(app: AppHandle) {
    // Muestra la ventana de login (creada oculta al arranque en modo real).
    // El usuario inicia sesión manualmente (incluido 2FA); la app solo
    // conserva la cookie que persiste el webview (SPEC §3).
    if let Some(win) = app.get_webview_window("login") {
        let _ = win.eval("window.location.href='https://claude.ai/login'");
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Menú del icono de bandeja — la única puerta de entrada al programa
/// (los overlays no abren ventanas).
fn build_tray_menu(app: &AppHandle, cfg: &Config, paused: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let n_mons = app
        .get_webview_window("bar")
        .and_then(|w| w.available_monitors().ok())
        .map(|m| m.len())
        .unwrap_or(1);
    let open = MenuItem::with_id(app, "open", "Abrir ClaudeClock", true, None::<&str>)?;
    let ocfg = MenuItem::with_id(app, "opencfg", "Configuración", true, None::<&str>)?;
    let mon1 = CheckMenuItem::with_id(app, "mon-1", "Monitor 1", true, cfg.monitor == "1", None::<&str>)?;
    let mon2 = CheckMenuItem::with_id(app, "mon-2", "Monitor 2", n_mons > 1, cfg.monitor == "2", None::<&str>)?;
    let monc = CheckMenuItem::with_id(app, "mon-cursor", "Seguir al cursor", true, cfg.monitor == "cursor", None::<&str>)?;
    let sub = Submenu::with_items(app, "Monitor", true, &[&mon1, &mon2, &monc])?;
    let ct = CheckMenuItem::with_id(app, "clickthrough", "Clic-through", true, cfg.click_through, None::<&str>)?;
    let pa = CheckMenuItem::with_id(app, "pause", "Pausar actualización", true, paused, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&open, &ocfg, &sep1, &sub, &ct, &pa, &sep2, &sep3, &quit])
}

/// Reconstruye el menú de la bandeja tras un cambio de estado (checks al día).
fn refresh_tray(app: &AppHandle) {
    let state = app.state::<AppState>();
    let cfg = state.cfg.lock().unwrap().clone();
    let paused = state.paused.load(Ordering::Relaxed);
    if let Some(tray) = app.tray_by_id("tray") {
        if let Ok(menu) = build_tray_menu(app, &cfg, paused) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn handle_menu(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    match id {
        "open" => open_window(app, "main"),
        "opencfg" => open_window(app, "settings"),
        "quit" => {
            state.history.lock().unwrap().flush(now_ts());
            app.exit(0);
        }
        "clickthrough" => {
            let cfg = {
                let mut c = state.cfg.lock().unwrap();
                c.click_through = !c.click_through;
                c.clone()
            };
            config::save(&state.cfg_path, &cfg);
            if let Some(bar) = app.get_webview_window("bar") {
                let _ = bar.set_ignore_cursor_events(cfg.click_through);
            }
            let _ = app.emit("config-changed", &cfg);
        }
        "pause" => {
            let p = !state.paused.load(Ordering::Relaxed);
            state.paused.store(p, Ordering::Relaxed);
        }
        m if m.starts_with("mon-") => {
            let cfg = {
                let mut c = state.cfg.lock().unwrap();
                c.monitor = m.trim_start_matches("mon-").to_string();
                c.clone()
            };
            config::save(&state.cfg_path, &cfg);
            reposition(app, &state, &cfg);
            let _ = app.emit("config-changed", &cfg);
        }
        _ => {}
    }
    if matches!(id, "clickthrough" | "pause") || id.starts_with("mon-") {
        refresh_tray(app);
    }
}

/* ============================ bucles de fondo ============================ */

/// Convierte un snapshot en payload con detección de actividad (SPEC §3.4).
fn make_payload(state: &AppState, s: f64, w: f64, f: f64, reset_s: i64, reset_w: i64, now: i64) -> UsagePayload {
    let prev = state.usage.lock().unwrap().clone();
    let act = |new: f64, old: f64| new > old + 0.01;
    let (ps, pw, pf) = prev.map(|p| (p.s, p.w, p.f)).unwrap_or((s, w, f));
    UsagePayload {
        s,
        w,
        f,
        reset_s,
        reset_w,
        act_s: act(s, ps),
        act_w: act(w, pw),
        act_f: act(f, pf),
        ts: now,
    }
}

fn store_and_emit(app: &AppHandle, p: UsagePayload, now: i64) {
    let state = app.state::<AppState>();
    if state.cfg.lock().unwrap().historial_on {
        state
            .history
            .lock()
            .unwrap()
            .push(Point { ts: now, s: p.s, w: p.w, f: p.f }, now);
    }
    *state.usage.lock().unwrap() = Some(p.clone());
    let _ = app.emit("usage", &p);
}

/// Poll de consumo (SPEC §3): demo cada 3 s; real según intervalo configurado,
/// con backoff exponencial ante errores y estado "reconectar" en 401.
fn spawn_poll_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut fails: u32 = 0;
        let mut first_real = true;
        loop {
            let state = app.state::<AppState>();
            let paused = state.paused.load(Ordering::Relaxed);
            if !paused {
                let now = now_ts();
                if state.demo {
                    let snap = state.demo_src.lock().unwrap().fetch(now);
                    let p = make_payload(&state, snap.s, snap.w, snap.f, snap.reset_s, snap.reset_w, now);
                    store_and_emit(&app, p, now);
                } else {
                    match api::poll(&app, now).await {
                        Ok((u, src)) => {
                            fails = 0;
                            let src = match src {
                                api::Source::Token => "token",
                                api::Source::Cookie => "cookie",
                            };
                            set_connected(&app, true, src);
                            let p = make_payload(&state, u.s, u.w, u.f, u.reset_s, u.reset_w, now);
                            store_and_emit(&app, p, now);
                            if first_real && !state.autostart {
                                // sesión viva desde el arranque: directo al dashboard
                                // (en autoarranque solo aparece el overlay)
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                }
                            }
                        }
                        Err(api::ApiError::NeedLogin) => {
                            fails = fails.saturating_add(1);
                            set_connected(&app, false, "");
                            if first_real && !state.autostart {
                                // sin token de Claude Code ni cookie: login manual;
                                // al cerrarlo, el dashboard (en autoarranque, nada:
                                // la barra queda en RECONECTAR y se entra por la bandeja)
                                if let Some(w) = app.get_webview_window("login") {
                                    let _ = w.eval("window.location.href='https://claude.ai/login'");
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                        }
                        Err(api::ApiError::Endpoint(p)) => {
                            eprintln!("[claudeclock] endpoint no válido: {p} — actualízalo en config.json");
                            fails = fails.saturating_add(1);
                            set_connected(&app, false, "");
                        }
                        Err(api::ApiError::Other(e)) => {
                            eprintln!("[claudeclock] error de poll: {e}");
                            fails = fails.saturating_add(1);
                        }
                    }
                    first_real = false;
                }
            }
            let base = if app.state::<AppState>().demo {
                3
            } else {
                app.state::<AppState>().cfg.lock().unwrap().intervalo.clamp(30, 300) as u64
            };
            // backoff exponencial: base·2^fails, tope 15 min (SPEC §3.3)
            let secs = if fails > 0 {
                (base * 2u64.saturating_pow(fails.min(5))).min(900)
            } else {
                base
            };
            let state = app.state::<AppState>();
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(secs)) => {},
                _ = state.notify.notified() => {},
            }
        }
    });
}

/// Vigilante cada 2 s: auto-ocultar en pantalla completa y modo "seguir al cursor".
fn spawn_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut hidden = false;
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let Some(bar) = app.get_webview_window("bar") else { continue };
            let state = app.state::<AppState>();

            let cfg = state.cfg.lock().unwrap().clone();
            if cfg.monitor == "cursor" {
                reposition(&app, &state, &cfg);
            }

            let mon = *state.bar_mon.lock().unwrap();
            let hwnd = bar.hwnd().map(|h| h.0 as isize).unwrap_or(0);
            let fs = barpos::fullscreen_covering(mon, hwnd);
            if fs != hidden {
                hidden = fs;
                if fs {
                    let _ = bar.hide();
                } else {
                    let _ = bar.show();
                }
            }
        }
    });
}

/* ============================ arranque ============================ */

fn main() {
    let demo = std::env::args().any(|a| a == "--demo");
    let show_settings = std::env::args().any(|a| a == "--show-settings");
    let autostart = std::env::args().any(|a| a == "--autostart");

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            boot,
            set_config,
            resize_bar,
            get_graph,
            get_projection,
            open_main,
            open_settings,
            clear_history,
            relogin,
        ])
        .setup(move |app| {
            let now = now_ts();
            let cfg_path = app
                .path()
                .app_config_dir()
                .expect("app_config_dir")
                .join("config.json");
            let cfg = config::load(&cfg_path);

            // historial: JSONL en disco; en demo solo RAM para no contaminar datos reales
            let hist_file = if demo {
                None
            } else {
                app.path().app_data_dir().ok().map(|d| d.join("history.jsonl"))
            };
            let mut history = History::load(hist_file, now);

            let demo_src = DemoSource::new(now);
            if demo {
                demo_src.seed_history(&mut history, now);
            }

            // sincroniza el registro de Windows con la preferencia guardada
            apply_autostart(cfg.autoarranque);

            app.manage(AppState {
                demo,
                autostart,
                cfg: Mutex::new(cfg.clone()),
                cfg_path,
                usage: Mutex::new(None),
                history: Mutex::new(history),
                demo_src: Mutex::new(demo_src),
                paused: AtomicBool::new(false),
                bar_mon: Mutex::new((0, 0, 0, 0)),
                bar_w: Mutex::new(None),
                org: Mutex::new(None),
                oauth: Mutex::new(None),
                source: Mutex::new(String::new()),
                connected: AtomicBool::new(false),
                notify: tokio::sync::Notify::new(),
            });

            // barra overlay: sin marco, always-on-top, sin Alt-Tab ni taskbar (SPEC §4)
            let bar = WebviewWindowBuilder::new(app, "bar", WebviewUrl::App("bar.html".into()))
                .title("ClaudeClock")
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focused(false)
                .shadow(false)
                .visible(false)
                .build()?;
            let state = app.state::<AppState>();
            if let Some(rect) = barpos::position_bar(&bar, &cfg, None) {
                *state.bar_mon.lock().unwrap() = rect;
            }
            barpos::make_noactivate(&bar);
            if cfg.click_through {
                let _ = bar.set_ignore_cursor_events(true);
            }
            let _ = bar.show();

            // Ventana ClaudeClock y Configuración: creadas ya, mostradas bajo demanda.
            // En demo el dashboard aparece de inmediato; en real, al cerrar el login.
            let main_win = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("main.html".into()))
                .title("ClaudeClock")
                .inner_size(560.0, 620.0)
                .decorations(false)
                .resizable(false) // el contenido dicta el tamaño (auto-ajuste desde JS)
                .visible(demo)
                .build()?;
            let settings_win =
                WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
                    .title("Configuración")
                    .inner_size(660.0, 720.0)
                    .decorations(false)
                    .resizable(false)
                    .visible(show_settings)
                    .build()?;
            // cerrar (✕ o close del JS) = ocultar; minimizar = ocultar a la bandeja
            for w in [&main_win, &settings_win] {
                let wc = w.clone();
                w.on_window_event(move |e| match e {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = wc.hide();
                    }
                    tauri::WindowEvent::Resized(_) => {
                        if wc.is_minimized().unwrap_or(false) {
                            let _ = wc.unminimize();
                            let _ = wc.hide();
                        }
                    }
                    _ => {}
                });
            }
            if !demo {
                // ventana de login: creada oculta (about:blank hasta necesitarla,
                // para no cargar claude.ai en RAM); se muestra solo sin sesión
                let login = WebviewWindowBuilder::new(
                    app,
                    "login",
                    WebviewUrl::External("about:blank".parse().unwrap()),
                )
                .title("Iniciar sesión en claude.ai")
                .inner_size(480.0, 720.0)
                .visible(false)
                .build()?;
                let lh = login.clone();
                let mh = main_win.clone();
                let ah = app.handle().clone();
                login.on_window_event(move |e| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                        api.prevent_close();
                        let _ = lh.eval("window.location.href='about:blank'"); // liberar RAM
                        let _ = lh.hide();
                        let _ = mh.show();
                        let _ = mh.set_focus();
                        // poll inmediato para validar la sesión recién iniciada
                        ah.state::<AppState>().notify.notify_one();
                    }
                });
            }

            app.on_menu_event(|app, event| handle_menu(app, event.id().as_ref()));

            // icono en la bandeja del sistema: única puerta de entrada al programa
            let tray_menu = build_tray_menu(app.handle(), &cfg, false)?;
            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().expect("icono").clone())
                .tooltip("ClaudeClock")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        open_window(tray.app_handle(), "main");
                    }
                })
                .build(app)?;

            spawn_poll_loop(app.handle().clone());
            spawn_watcher(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error al iniciar ClaudeClock")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // volcar el buffer de historial pendiente (SPEC §8)
                app.state::<AppState>().history.lock().unwrap().flush(now_ts());
            }
        });
}
