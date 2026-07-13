//! Datos de consumo. Fuente demo (--demo) con los mismos datos de las maquetas;
//! la fuente real (sesión claude.ai) se conecta en una fase posterior (SPEC §3).

use crate::history::{History, Point};
use serde::Serialize;

/// Lo que se emite al frontend en cada poll.
#[derive(Clone, Serialize)]
pub struct UsagePayload {
    pub s: f64,
    pub w: f64,
    pub f: f64,
    pub reset_s: i64,
    pub reset_w: i64,
    pub act_s: bool,
    pub act_w: bool,
    pub act_f: bool,
    pub ts: i64,
}

pub struct Snapshot {
    pub s: f64,
    pub w: f64,
    pub f: f64,
    pub reset_s: i64,
    pub reset_w: i64,
}

/// Deltas por bucket de las maquetas (suman exactamente 42 / 67 / 18).
const DS: [f64; 10] = [3.0, 5.0, 4.0, 6.0, 8.0, 5.0, 4.0, 3.0, 2.0, 2.0];
const DW: [f64; 28] = [
    2.0, 3.0, 1.0, 2.0, 3.0, 4.0, 2.0, 3.0, 1.0, 2.0, 3.0, 2.0, 4.0, 3.0,
    2.0, 3.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0, 3.0, 2.0, 2.0, 2.0, 2.0, 1.0,
];
const DF: [f64; 28] = [
    0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0,
    0.0, 1.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0,
];

pub const WSTEP: i64 = 21_600; // 6 h
pub const SSTEP: i64 = 1_800; // 30 min

pub struct DemoSource {
    s: f64,
    w: f64,
    f: f64,
    reset_s: i64,
    reset_w: i64,
    tick: u64,
    seed: u64,
}

impl DemoSource {
    pub fn new(now: i64) -> Self {
        Self {
            s: 42.0,
            w: 67.0,
            f: 18.0,
            reset_s: now + 2 * 3600 + 14 * 60 + 7, // "reinicia en 02:14:07"
            reset_w: now + 4 * 86_400 + 3 * 3600,  // "reinicia en 4 d 3 h"
            tick: 0,
            seed: 0x9E3779B97F4A7C15,
        }
    }

    /// Siembra el historial con la semana/sesión de las maquetas para que
    /// ClaudeGraph tenga datos desde el primer arranque en demo.
    pub fn seed_history(&self, hist: &mut History, now: i64) {
        // eventos (ts, medidor, delta) → puntos acumulados en orden cronológico
        let mut events: Vec<(i64, usize, f64)> = Vec::new();
        let wstart = (now / WSTEP) * WSTEP - 27 * WSTEP;
        for i in 0..28 {
            let ts = (wstart + i as i64 * WSTEP + 120).min(now - 1);
            if DW[i] > 0.0 {
                events.push((ts, 1, DW[i]));
            }
            if DF[i] > 0.0 {
                events.push((ts + 30, 2, DF[i]));
            }
        }
        // sesión: los 10 deltas comprimidos en el tramo transcurrido de la ventana de 5 h
        let sstart = self.reset_s - 5 * 3600;
        let elapsed = (now - sstart).max(600);
        for i in 0..10 {
            let ts = sstart + elapsed * (2 * i as i64 + 1) / 20;
            events.push((ts.min(now - 1), 0, DS[i]));
        }
        events.sort_by_key(|e| e.0);

        hist.push(Point { ts: wstart - 60, s: 0.0, w: 0.0, f: 0.0 }, now);
        let (mut s, mut w, mut f) = (0.0, 0.0, 0.0);
        for (ts, m, d) in events {
            match m {
                0 => s += d,
                1 => w += d,
                _ => f += d,
            }
            hist.push(Point { ts, s, w, f }, now);
        }
    }

    fn rnd(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed % 1000) as f64 / 1000.0
    }

    /// Un "poll" simulado. Ciclo de 8 ticks: reposo → actividad → reposo → actividad con Fable,
    /// como el auto-demo de la maqueta.
    pub fn fetch(&mut self, now: i64) -> Snapshot {
        if now >= self.reset_s {
            self.s = 0.0;
            self.reset_s += 5 * 3600;
        }
        if now >= self.reset_w {
            self.w = 0.0;
            self.f = 0.0;
            self.reset_w += 7 * 86_400;
        }
        let phase = self.tick % 8;
        self.tick += 1;
        match phase {
            0 | 1 | 6 => {} // reposo
            7 => {
                // actividad con Fable
                self.f = (self.f + 0.25 + self.rnd() * 0.3).min(100.0);
                self.s = (self.s + 0.3 + self.rnd() * 0.4).min(100.0);
                self.w = (self.w + 0.1 + self.rnd() * 0.15).min(100.0);
            }
            _ => {
                // actividad normal (Sesión + Semanal)
                self.s = (self.s + 0.4 + self.rnd() * 0.5).min(100.0);
                self.w = (self.w + 0.15 + self.rnd() * 0.2).min(100.0);
            }
        }
        Snapshot {
            s: self.s,
            w: self.w,
            f: self.f,
            reset_s: self.reset_s,
            reset_w: self.reset_w,
        }
    }
}
