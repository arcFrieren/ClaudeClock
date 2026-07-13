use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Totales {
    pub s: u32,
    pub w: u32,
    pub f: u32,
}

impl Default for Totales {
    fn default() -> Self {
        Self { s: 10_000, w: 20_000, f: 5_000 }
    }
}

/// Config persistente (SPEC §7).
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub tema: String,
    pub fondo: String,
    /// tamaño de barra: "fino" | "compacto"
    pub alto_modo: String,
    /// posición: "izq-sup" | "med-sup" | "der-sup" | "izq-inf" | "med-inf" | "der-inf"
    pub posicion: String,
    pub monitor: String,
    pub intervalo: u32,
    pub cr: bool,
    pub cr_avisado: bool,
    pub graph_on: bool,
    pub graph_avisado: bool,
    pub graph_en_principal: bool,
    pub totales: Totales,
    pub historial_on: bool,
    pub autoarranque: bool,
    pub click_through: bool,
    pub endpoint: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tema: "geist".into(),
            fondo: "sin".into(),
            alto_modo: "fino".into(),
            posicion: "der-sup".into(),
            monitor: "1".into(),
            intervalo: 60,
            cr: false,
            cr_avisado: false,
            graph_on: false,
            graph_avisado: false,
            graph_en_principal: false,
            totales: Totales::default(),
            historial_on: true,
            autoarranque: true,
            click_through: false,
            endpoint: String::new(),
        }
    }
}

pub fn load(path: &Path) -> Config {
    let mut cfg: Config = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    // migración del esquema viejo "fino-izq"/"compacto-der" → tamaño + posición
    if cfg.alto_modo.contains('-') {
        let der = cfg.alto_modo.ends_with("der");
        cfg.alto_modo = if cfg.alto_modo.starts_with("fino") { "fino" } else { "compacto" }.into();
        cfg.posicion = if der { "der-inf" } else { "izq-inf" }.into();
    }
    if cfg.posicion.is_empty() {
        cfg.posicion = "der-sup".into();
    }
    cfg
}

pub fn save(path: &Path, cfg: &Config) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, json);
    }
}
