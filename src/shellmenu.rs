//! "Copy tree with Twig" in File Explorer's right-click menu, for folders, the
//! inside of a folder and drives. A few keys under HKCU\Software\Classes: no
//! shell extension, nothing loaded into Explorer, nothing running until you
//! click it. On Windows 11 it sits under "Show more options" (or Shift+right-click).

use crate::win::wide;
use std::ptr::null_mut;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW,
};

pub const LABEL: &str = "Copy tree with Twig";
const PLACES: [&str; 3] = [r"Directory\shell\Twig", r"Directory\Background\shell\Twig", r"Drive\shell\Twig"];

fn key(place: &str) -> String {
    format!(r"Software\Classes\{place}")
}

fn exe() -> Option<String> {
    std::env::current_exe().ok().map(|p| p.display().to_string())
}

fn command() -> Option<String> {
    exe().map(|e| format!("\"{e}\" --copy \"%V\""))
}

fn set(key: &str, name: Option<&str>, value: &str) {
    let (k, data) = (wide(key), wide(value));
    let n = name.map(wide);
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            n.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        );
    }
}

fn get(key: &str) -> Option<String> {
    let k = wide(key);
    let mut buf = vec![0u16; 2048];
    let mut bytes = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            std::ptr::null(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    (rc == 0).then(|| crate::win::from_wide(&buf))
}

pub fn is_installed() -> bool {
    get(&format!(r"{}\command", key(PLACES[0]))).is_some()
}

pub fn install() {
    let (Some(cmd), Some(exe)) = (command(), exe()) else { return };
    for place in PLACES {
        let k = key(place);
        set(&k, None, LABEL);
        set(&k, Some("Icon"), &format!("\"{exe}\",0"));
        // One folder at a time: several selected would each replace the last on the clipboard.
        set(&k, Some("MultiSelectModel"), "Single");
        set(&format!(r"{k}\command"), None, &cmd);
    }
}

pub fn remove() {
    for place in PLACES {
        let k = wide(&key(place));
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, k.as_ptr()) };
    }
}

/// If the menu entry points at a Twig that moved, point it at this copy.
pub fn follow_moves() {
    if let (Some(have), Some(want)) = (get(&format!(r"{}\command", key(PLACES[0]))), command()) {
        if !have.eq_ignore_ascii_case(&want) {
            install();
        }
    }
}
