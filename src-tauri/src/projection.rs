//! Proyección de agotamiento (SPEC §6): corre en Rust, no en el webview.
//! Regresión simple: ritmo de las últimas N horas contra el % restante.

use chrono::{Local, TimeZone, Timelike};

const DIAS: [&str; 7] = [
    "lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo",
];

/// Devuelve "jueves-14:00" (siempre nombre de día, nunca "hoy") o None si no hay ritmo.
pub fn project(current_pct: f64, rate_per_hour: f64, now: i64) -> Option<String> {
    if rate_per_hour <= 0.01 || current_pct >= 100.0 {
        return None;
    }
    let hours_left = (100.0 - current_pct) / rate_per_hour;
    if hours_left > 24.0 * 30.0 {
        return None; // demasiado lejos para ser útil
    }
    let t = now + (hours_left * 3600.0) as i64;
    let dt = Local.timestamp_opt(t, 0).single()?;
    use chrono::Datelike;
    let dia = DIAS[dt.weekday().num_days_from_monday() as usize];
    Some(format!("{}-{:02}:{:02}", dia, dt.hour(), dt.minute()))
}
