//! Posicionamiento de la barra sobre la taskbar y utilidades Win32
//! (área de trabajo por monitor, no-activación, detección de pantalla completa).

use crate::config::Config;
use tauri::{Monitor, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

#[cfg(windows)]
mod win {
    pub use windows_sys::Win32::Foundation::{POINT, RECT};
    pub use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO,
        MONITOR_DEFAULTTONEAREST,
    };
    pub use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };
}

pub type Rect = (i32, i32, i32, i32); // left, top, right, bottom

/// rcMonitor y rcWork del monitor que contiene el punto (px físicos).
#[cfg(windows)]
fn monitor_rects(cx: i32, cy: i32) -> Option<(Rect, Rect)> {
    unsafe {
        let hm = win::MonitorFromPoint(win::POINT { x: cx, y: cy }, win::MONITOR_DEFAULTTONEAREST);
        let mut mi: win::MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<win::MONITORINFO>() as u32;
        if win::GetMonitorInfoW(hm, &mut mi) == 0 {
            return None;
        }
        let m = mi.rcMonitor;
        let w = mi.rcWork;
        Some(((m.left, m.top, m.right, m.bottom), (w.left, w.top, w.right, w.bottom)))
    }
}

fn pick_monitor<R: Runtime>(win: &WebviewWindow<R>, cfg: &Config) -> Option<Monitor> {
    let mons = win.available_monitors().ok()?;
    if mons.is_empty() {
        return None;
    }
    if cfg.monitor == "cursor" {
        if let Ok(c) = win.cursor_position() {
            let (cx, cy) = (c.x as i32, c.y as i32);
            for m in &mons {
                let p = m.position();
                let s = m.size();
                if cx >= p.x && cx < p.x + s.width as i32 && cy >= p.y && cy < p.y + s.height as i32 {
                    return Some(m.clone());
                }
            }
        }
        return mons.first().cloned();
    }
    let idx = cfg.monitor.parse::<usize>().unwrap_or(1).saturating_sub(1);
    mons.get(idx).cloned().or_else(|| mons.first().cloned())
}

/// Coloca la barra anclada al borde superior del área de trabajo de la taskbar
/// del monitor configurado (SPEC §4). Devuelve el rect del monitor usado.
/// `compact_w`: ancho medido del contenido (px) para los modos Compacto.
pub fn position_bar<R: Runtime>(
    win: &WebviewWindow<R>,
    cfg: &Config,
    compact_w: Option<u32>,
) -> Option<Rect> {
    let mon = pick_monitor(win, cfg)?;
    let p = mon.position();
    let s = mon.size();
    let (mw, mh) = (s.width as i32, s.height as i32);

    #[cfg(windows)]
    let (mrect, wrect) =
        monitor_rects(p.x + mw / 2, p.y + mh / 2).unwrap_or((
            (p.x, p.y, p.x + mw, p.y + mh),
            (p.x, p.y, p.x + mw, p.y + mh - 48),
        ));
    #[cfg(not(windows))]
    let (mrect, wrect) = ((p.x, p.y, p.x + mw, p.y + mh), (p.x, p.y, p.x + mw, p.y + mh - 48));

    // alto real de la taskbar (abajo); si está oculta o lateral, fallback 48 px
    let taskbar_h = {
        let h = mrect.3 - wrect.3;
        if h <= 0 { 48 } else { h }
    };
    let bar_h = (taskbar_h / 2).max(16);

    let fino = cfg.alto_modo.starts_with("fino");
    let bar_w = if fino {
        mw
    } else {
        // Compacto: lo más reducido posible — el ancho real del contenido
        compact_w
            .map(|w| (w as i32).clamp(160, mw))
            .unwrap_or((mw as f64 * 0.45) as i32)
    };
    // posición: izq/med/der × sup/inf
    let x = if cfg.posicion.starts_with("der") {
        mrect.2 - bar_w
    } else if cfg.posicion.starts_with("med") {
        mrect.0 + (mw - bar_w) / 2
    } else {
        mrect.0
    };
    let y = if cfg.posicion.ends_with("sup") {
        wrect.1 // borde superior del área de trabajo
    } else {
        wrect.3 - bar_h // pegada al borde superior de la taskbar
    };

    let _ = win.set_size(PhysicalSize::new(bar_w as u32, bar_h as u32));
    let _ = win.set_position(PhysicalPosition::new(x, y));
    Some(mrect)
}

/// WS_EX_NOACTIVATE: la barra recibe clics pero nunca roba el foco (SPEC §10).
#[cfg(windows)]
pub fn make_noactivate<R: Runtime>(win: &WebviewWindow<R>) {
    if let Ok(hwnd) = win.hwnd() {
        unsafe {
            let h = hwnd.0 as windows_sys::Win32::Foundation::HWND;
            let ex = win::GetWindowLongPtrW(h, win::GWL_EXSTYLE);
            win::SetWindowLongPtrW(h, win::GWL_EXSTYLE, ex | win::WS_EX_NOACTIVATE as isize);
        }
    }
}
#[cfg(not(windows))]
pub fn make_noactivate<R: Runtime>(_win: &WebviewWindow<R>) {}

/// ¿Hay una app en pantalla completa tapando el monitor de la barra? (SPEC §4 auto-ocultar)
#[cfg(windows)]
pub fn fullscreen_covering(mon: Rect, bar_hwnd: isize) -> bool {
    unsafe {
        let fg = win::GetForegroundWindow();
        if fg.is_null() || fg as isize == bar_hwnd {
            return false;
        }
        let mut cls = [0u16; 64];
        let n = win::GetClassNameW(fg, cls.as_mut_ptr(), 64);
        if n > 0 {
            let name = String::from_utf16_lossy(&cls[..n as usize]);
            if matches!(name.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
                return false;
            }
        }
        let mut r: win::RECT = std::mem::zeroed();
        if win::GetWindowRect(fg, &mut r) == 0 {
            return false;
        }
        let hm = win::MonitorFromWindow(fg, win::MONITOR_DEFAULTTONEAREST);
        let mut mi: win::MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<win::MONITORINFO>() as u32;
        if win::GetMonitorInfoW(hm, &mut mi) == 0 {
            return false;
        }
        let m = mi.rcMonitor;
        let same = m.left == mon.0 && m.top == mon.1 && m.right == mon.2 && m.bottom == mon.3;
        same && r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}
#[cfg(not(windows))]
pub fn fullscreen_covering(_mon: Rect, _bar_hwnd: isize) -> bool {
    false
}
