//! Small Win32 helpers shared by the resident and the settings window.

use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MONITORINFOEXW,
    MonitorFromPoint,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, GetCursorPos, MB_ICONINFORMATION, MB_OK, MessageBoxW, SW_SHOWNORMAL,
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn from_wide(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

pub fn open_url(url: &str) {
    let (verb, url) = (wide("open"), wide(url));
    unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), url.as_ptr(), null(), null(), SW_SHOWNORMAL) };
}

pub fn open_path(path: &Path) {
    open_url(&path.display().to_string());
}

/// Starts another copy of this exe with `args` (the settings window, the hello card).
pub fn spawn_self(args: &[&str]) {
    if let Ok(exe) = std::env::current_exe() {
        // Let the new window come to the front: we were the one the user clicked.
        unsafe { AllowSetForegroundWindow(u32::MAX) };
        let _ = std::process::Command::new(exe).args(args).spawn();
    }
}

pub fn message(title: &str, text: &str) {
    let (title, text) = (wide(title), wide(text));
    unsafe { MessageBoxW(null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONINFORMATION) };
}

pub fn cursor() -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    p
}

#[derive(Clone)]
pub struct Monitor {
    pub handle: HMONITOR,
    /// The part not covered by the taskbar.
    pub work: RECT,
}

impl Monitor {
    /// Effective DPI scale (1.0 = 96 dpi).
    pub fn scale(&self) -> f32 {
        if let Some(scale) = test_scale() {
            return scale;
        }
        let (mut x, mut y) = (96u32, 96u32);
        unsafe { GetDpiForMonitor(self.handle, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
        x.max(96) as f32 / 96.0
    }
}

/// A scale-test build's stand-in for Windows' display scale.
pub fn test_scale() -> Option<f32> {
    if !cfg!(feature = "scale-test") {
        return None;
    }
    std::env::var("TWIG_TEST_SCALE").ok()?.parse().ok()
}

pub fn monitor_info(handle: HMONITOR) -> Option<Monitor> {
    let mut mi: MONITORINFOEXW = unsafe { std::mem::zeroed() };
    mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(handle, &mut mi as *mut _ as *mut MONITORINFO) } == 0 {
        return None;
    }
    Some(Monitor { handle, work: mi.monitorInfo.rcWork })
}

pub fn monitor_at(p: POINT) -> Option<Monitor> {
    monitor_info(unsafe { MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST) })
}

pub fn primary_monitor() -> Option<Monitor> {
    monitor_info(unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) })
}

/// The HWND of a window or null, as an Option.
pub fn some(h: HWND) -> Option<HWND> {
    (!h.is_null()).then_some(h)
}

pub fn contains(r: &RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}
