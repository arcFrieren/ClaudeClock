//! Historial JSONL append-only con buffer en RAM (SPEC §8).
//! Un punto por poll; el buffer se vuelca a disco cada 5 min y al salir.
//! Compactación al arrancar: lo más viejo que 7 días queda a resolución de 30 min.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

const FLUSH_SECS: i64 = 300;
const WEEK: i64 = 7 * 86_400;
const COMPACT_STEP: i64 = 1800;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub ts: i64,
    pub s: f64,
    pub w: f64,
    pub f: f64,
}

pub struct History {
    /// `None` en modo demo: todo vive en RAM y no se toca el disco.
    file: Option<PathBuf>,
    points: Vec<Point>,
    pending: Vec<Point>,
    last_flush: i64,
}

impl History {
    pub fn load(file: Option<PathBuf>, now: i64) -> Self {
        let mut points: Vec<Point> = Vec::new();
        if let Some(p) = &file {
            if let Ok(txt) = std::fs::read_to_string(p) {
                points = txt
                    .lines()
                    .filter_map(|l| serde_json::from_str(l).ok())
                    .collect();
            }
        }
        let mut h = Self { file, points, pending: Vec::new(), last_flush: now };
        h.compact(now);
        h
    }

    /// Compacta lo anterior a 7 días a un punto por cada 30 min y reescribe el archivo.
    fn compact(&mut self, now: i64) {
        let cutoff = now - WEEK;
        let old_len = self.points.len();
        let mut compacted: Vec<Point> = Vec::new();
        let mut bucket: Option<i64> = None;
        for p in &self.points {
            if p.ts >= cutoff {
                compacted.push(*p);
                continue;
            }
            let b = p.ts / COMPACT_STEP;
            if bucket == Some(b) {
                *compacted.last_mut().unwrap() = *p; // conserva el último del bucket
            } else {
                compacted.push(*p);
                bucket = Some(b);
            }
        }
        if compacted.len() != old_len {
            self.points = compacted;
            self.rewrite();
        }
    }

    fn rewrite(&self) {
        let Some(path) = &self.file else { return };
        let mut out = String::new();
        for p in &self.points {
            out.push_str(&serde_json::to_string(p).unwrap());
            out.push('\n');
        }
        let _ = std::fs::write(path, out);
    }

    pub fn push(&mut self, p: Point, now: i64) {
        self.points.push(p);
        if self.file.is_some() {
            self.pending.push(p);
            if now - self.last_flush >= FLUSH_SECS {
                self.flush(now);
            }
        }
    }

    pub fn flush(&mut self, now: i64) {
        self.last_flush = now;
        if self.pending.is_empty() {
            return;
        }
        let Some(path) = &self.file else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            for p in &self.pending {
                let _ = writeln!(f, "{}", serde_json::to_string(p).unwrap());
            }
        }
        self.pending.clear();
    }

    pub fn clear(&mut self) {
        self.points.clear();
        self.pending.clear();
        if let Some(p) = &self.file {
            let _ = std::fs::remove_file(p);
        }
    }

    fn val(p: &Point, meter: &str) -> f64 {
        match meter {
            "s" => p.s,
            "w" => p.w,
            _ => p.f,
        }
    }

    /// Suma de deltas positivos del medidor por bucket (puntos de % consumidos).
    pub fn buckets(&self, meter: &str, start: i64, step: i64, n: usize) -> Vec<f64> {
        let mut out = vec![0.0; n];
        let mut prev: Option<f64> = None;
        for p in &self.points {
            let v = Self::val(p, meter);
            if p.ts < start {
                prev = Some(v);
                continue;
            }
            let idx = ((p.ts - start) / step) as usize;
            if idx >= n {
                break;
            }
            if let Some(pv) = prev {
                let d = v - pv;
                if d > 0.0 {
                    out[idx] += d;
                }
            }
            prev = Some(v);
        }
        out
    }

    /// Ritmo de consumo en puntos de % por hora durante las últimas `hours` horas.
    pub fn rate_per_hour(&self, meter: &str, now: i64, hours: f64) -> f64 {
        let cutoff = now - (hours * 3600.0) as i64;
        let mut sum = 0.0;
        let mut prev: Option<f64> = None;
        for p in &self.points {
            let v = Self::val(p, meter);
            if p.ts >= cutoff {
                if let Some(pv) = prev {
                    let d = v - pv;
                    if d > 0.0 {
                        sum += d;
                    }
                }
            }
            prev = Some(v);
        }
        sum / hours
    }
}
