//! Fuentes de datos de consumo (SPEC §3).
//!
//! 1) PRIMARIA — token OAuth local de Claude Code: se lee del perfil del
//!    usuario que ejecuta la app (`%USERPROFILE%\.claude\.credentials.json`,
//!    o `CLAUDE_CONFIG_DIR`). Sin login embebido: si Claude Code está logueado,
//!    ClaudeClock ya tiene datos. Se renueva con el refresh token si expiró
//!    (solo en memoria; nunca se escribe el archivo de Claude Code).
//! 2) RESPALDO — cookie de sesión de claude.ai del perfil WebView2 (login
//!    manual en la ventana embebida).

use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};

pub enum ApiError {
    /// sin token válido y sin cookie → estado "reconectar" + re-login
    NeedLogin,
    /// 404: la ruta interna cambió → actualizar `endpoint` en config.json
    Endpoint(String),
    Other(String),
}

#[derive(Clone, Copy)]
pub enum Source {
    Token,
    Cookie,
}

pub struct RawUsage {
    pub s: f64,
    pub w: f64,
    pub f: f64,
    pub reset_s: i64,
    pub reset_w: i64,
}

const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
/// id de cliente OAuth público de Claude Code
const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const DEFAULT_ENDPOINT: &str = "/api/organizations/{org_id}/usage";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 Edg/126.0.0.0";

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(UA)
            .build()
            .expect("cliente http")
    })
}

/* ==================== parse compartido ==================== */

/// utilization puede venir 0–1 o 0–100 según la versión del endpoint
fn pct(v: &Value) -> f64 {
    let u = v
        .get("utilization")
        .or_else(|| v.get("used_pct"))
        .or_else(|| v.get("percentage"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let u = if u <= 1.0 { u * 100.0 } else { u };
    u.clamp(0.0, 100.0)
}

fn reset_ts(v: &Value, fallback: i64) -> i64 {
    v.get("resets_at")
        .or_else(|| v.get("reset_at"))
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp())
        .unwrap_or(fallback)
}

/// Porcentaje desde el array `limits` [{kind, percent, scope:{model:{display_name}}}].
/// Coincide por `kind` o por el nombre del modelo del scope (p. ej. Fable/Opus).
fn limit_pct(root: &Value, kinds: &[&str], models: &[&str]) -> f64 {
    root.get("limits")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter().find_map(|l| {
                let kind_ok = l
                    .get("kind")
                    .and_then(Value::as_str)
                    .map(|k| kinds.contains(&k))
                    .unwrap_or(false);
                let model_ok = l
                    .pointer("/scope/model/display_name")
                    .and_then(Value::as_str)
                    .map(|d| models.iter().any(|m| d.eq_ignore_ascii_case(m)))
                    .unwrap_or(false);
                if kind_ok || model_ok {
                    l.get("percent").and_then(Value::as_f64)
                } else {
                    None
                }
            })
        })
        .unwrap_or(0.0)
        .clamp(0.0, 100.0)
}

fn parse_usage(v: &Value, now: i64) -> RawUsage {
    let root = v.get("usage").unwrap_or(v);
    let five = root.get("five_hour").or_else(|| root.get("session")).cloned().unwrap_or(Value::Null);
    let week = root.get("seven_day").or_else(|| root.get("weekly")).cloned().unwrap_or(Value::Null);
    let opus = root
        .get("seven_day_opus")
        .or_else(|| root.get("seven_day_sonnet_opus"))
        .or_else(|| root.get("opus"))
        .cloned()
        .unwrap_or(Value::Null);
    RawUsage {
        s: if five.is_null() { limit_pct(root, &["session", "five_hour"], &[]) } else { pct(&five) },
        w: if week.is_null() { limit_pct(root, &["weekly_all", "seven_day", "weekly"], &[]) } else { pct(&week) },
        // el límite del modelo superior (Fable/Opus) viene como límite semanal
        // "scoped" al modelo dentro de `limits`
        f: if opus.is_null() {
            limit_pct(root, &["weekly_scoped"], &["Fable", "Opus", "Mythos"])
        } else {
            pct(&opus)
        },
        reset_s: reset_ts(&five, now + 5 * 3600),
        reset_w: reset_ts(&week, now + 7 * 86_400),
    }
}

/* ==================== fuente 1: token de Claude Code ==================== */

struct CodeCreds {
    access: String,
    refresh: Option<String>,
    expires_ms: i64,
}

/// Rutas candidatas de credenciales del usuario ACTUAL (dinámicas, sin
/// rutas fijas): CLAUDE_CONFIG_DIR, ~/.claude, ~/.config/claude.
fn credentials_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        out.push(PathBuf::from(dir).join(".credentials.json"));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        out.push(PathBuf::from(&home).join(".claude").join(".credentials.json"));
        out.push(PathBuf::from(&home).join(".config").join("claude").join(".credentials.json"));
    }
    out
}

fn read_code_creds() -> Option<CodeCreds> {
    for path in credentials_paths() {
        let Ok(txt) = std::fs::read_to_string(&path) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&txt) else { continue };
        let o = v.get("claudeAiOauth").unwrap_or(&v);
        let Some(access) = o.get("accessToken").and_then(Value::as_str) else { continue };
        return Some(CodeCreds {
            access: access.to_string(),
            refresh: o.get("refreshToken").and_then(Value::as_str).map(String::from),
            expires_ms: o.get("expiresAt").and_then(Value::as_i64).unwrap_or(0),
        });
    }
    None
}

/// Renueva el access token (solo en memoria; no toca el archivo de Claude Code).
async fn refresh_token(refresh: &str) -> Option<(String, i64)> {
    let resp = client()
        .post(OAUTH_TOKEN_URL)
        .header("User-Agent", API_UA)
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh,
            "client_id": OAUTH_CLIENT_ID,
        }))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    let access = v.get("access_token").and_then(Value::as_str)?.to_string();
    let expires_in = v.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);
    let exp_ms = chrono::Local::now().timestamp_millis() + expires_in * 1000;
    Some((access, exp_ms))
}

/// La API de Anthropic rechaza User-Agents de navegador: UA propio y neutro.
const API_UA: &str = concat!("claudeclock/", env!("CARGO_PKG_VERSION"));

async fn oauth_usage(token: &str, now: i64) -> Result<RawUsage, ApiError> {
    let resp = client()
        .get(OAUTH_USAGE_URL)
        .header("User-Agent", API_UA)
        .bearer_auth(token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| ApiError::Other(e.to_string()))?;
    let status = resp.status().as_u16();
    match status {
        200 => {
            let v: Value = resp.json().await.map_err(|e| ApiError::Other(e.to_string()))?;
            Ok(parse_usage(&v, now))
        }
        401 | 403 => Err(ApiError::NeedLogin),
        404 => Err(ApiError::Endpoint(OAUTH_USAGE_URL.to_string())),
        s => Err(ApiError::Other(format!("HTTP {s}"))),
    }
}

async fn oauth_poll(app: &AppHandle, now: i64) -> Result<RawUsage, ApiError> {
    let state = app.state::<crate::AppState>();
    let now_ms = now * 1000;
    // se relee el archivo en cada poll: así seguimos los refresh del propio Claude Code
    let file = read_code_creds();

    let mut token = file
        .as_ref()
        .filter(|c| c.expires_ms > now_ms + 60_000)
        .map(|c| c.access.clone());
    if token.is_none() {
        // refresh propio anterior aún vigente
        token = state
            .oauth
            .lock()
            .unwrap()
            .clone()
            .filter(|(_, exp)| *exp > now_ms + 60_000)
            .map(|(t, _)| t);
    }
    if token.is_none() {
        if let Some(r) = file.as_ref().and_then(|c| c.refresh.clone()) {
            if let Some((t, exp)) = refresh_token(&r).await {
                *state.oauth.lock().unwrap() = Some((t.clone(), exp));
                token = Some(t);
            }
        }
    }
    let token = token.ok_or(ApiError::NeedLogin)?;

    match oauth_usage(&token, now).await {
        Err(ApiError::NeedLogin) => {
            // token rechazado: un intento de refresh y reintento único
            if let Some(r) = file.and_then(|c| c.refresh) {
                if let Some((t, exp)) = refresh_token(&r).await {
                    *state.oauth.lock().unwrap() = Some((t.clone(), exp));
                    return oauth_usage(&t, now).await;
                }
            }
            Err(ApiError::NeedLogin)
        }
        other => other,
    }
}

/* ==================== fuente 2: cookie de claude.ai ==================== */

async fn get_json(path: &str, cookie: &str) -> Result<Value, ApiError> {
    let resp = client()
        .get(format!("https://claude.ai{path}"))
        .header("Cookie", cookie)
        .header("Accept", "application/json")
        .header("Referer", "https://claude.ai/settings/usage")
        .send()
        .await
        .map_err(|e| ApiError::Other(e.to_string()))?;
    match resp.status().as_u16() {
        200 => resp.json().await.map_err(|e| ApiError::Other(e.to_string())),
        401 | 403 => Err(ApiError::NeedLogin),
        404 => Err(ApiError::Endpoint(path.to_string())),
        s => Err(ApiError::Other(format!("HTTP {s}"))),
    }
}

async fn cookie_poll(app: &AppHandle, now: i64) -> Result<RawUsage, ApiError> {
    let app2 = app.clone();
    let cookie = tauri::async_runtime::spawn_blocking(move || {
        crate::cookies::get_cookie_header(&app2)
    })
    .await
    .ok()
    .flatten()
    .ok_or(ApiError::NeedLogin)?;
    if !cookie.contains("sessionKey=") {
        return Err(ApiError::NeedLogin);
    }

    let state = app.state::<crate::AppState>();

    // organización (cacheada tras el primer descubrimiento)
    let cached = state.org.lock().unwrap().clone();
    let org = match cached {
        Some(o) => o,
        None => {
            let v = get_json("/api/organizations", &cookie).await?;
            let arr = v
                .as_array()
                .cloned()
                .or_else(|| v.get("data").and_then(Value::as_array).cloned())
                .unwrap_or_default();
            let uuid = arr
                .iter()
                .find_map(|o| o.get("uuid").and_then(Value::as_str).map(String::from))
                .ok_or_else(|| ApiError::Other("cuenta sin organizaciones".into()))?;
            *state.org.lock().unwrap() = Some(uuid.clone());
            uuid
        }
    };

    let tmpl = {
        let c = state.cfg.lock().unwrap();
        if c.endpoint.is_empty() {
            DEFAULT_ENDPOINT.to_string()
        } else {
            c.endpoint.clone()
        }
    };
    let v = get_json(&tmpl.replace("{org_id}", &org), &cookie).await?;
    Ok(parse_usage(&v, now))
}

/* ==================== orquestación ==================== */

/// Token de Claude Code primero; cookie del webview como respaldo.
/// Solo se degrada a la cookie cuando el token NO sirve (NeedLogin): un error
/// transitorio de red no debe convertirse en un falso "sin sesión"/RECONECTAR.
pub async fn poll(app: &AppHandle, now: i64) -> Result<(RawUsage, Source), ApiError> {
    match oauth_poll(app, now).await {
        Ok(u) => Ok((u, Source::Token)),
        Err(ApiError::NeedLogin) => cookie_poll(app, now).await.map(|u| (u, Source::Cookie)),
        Err(e) => Err(e),
    }
}
