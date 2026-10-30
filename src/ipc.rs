//! How the settings window, a second launch and `Twig.exe --copy` talk to the
//! running resident: window messages to its hidden window, found by class name.

use crate::win::{some, wide};
use std::ptr::null;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, PostMessageW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_APP, WM_COPYDATA,
};

pub const RESIDENT_CLASS: &str = "Twig.Resident";

/// Re-read settings.toml.
pub const WM_TWIG_RELOAD: u32 = WM_APP + 1;
/// Open the settings window.
pub const WM_TWIG_SETTINGS: u32 = WM_APP + 3;
/// Quit.
pub const WM_TWIG_QUIT: u32 = WM_APP + 4;
/// WM_COPYDATA's tag for "copy this folder's tree" (the path, UTF-16).
pub const COPY_TAG: usize = 0x7477_6967; // "twig"

pub fn resident() -> Option<HWND> {
    let class = wide(RESIDENT_CLASS);
    some(unsafe { FindWindowW(class.as_ptr(), null()) })
}

pub fn send(msg: u32, wparam: usize, lparam: isize) -> bool {
    resident().is_some_and(|h| unsafe { PostMessageW(h, msg, wparam, lparam) } != 0)
}

/// Hands a folder to the running resident. False if none is running (the
/// caller then copies it itself).
pub fn request_copy(path: &str) -> bool {
    let Some(hwnd) = resident() else { return false };
    let data: Vec<u16> = path.encode_utf16().collect();
    let cds = COPYDATASTRUCT { dwData: COPY_TAG, cbData: (data.len() * 2) as u32, lpData: data.as_ptr() as *mut _ };
    let mut result = 0usize;
    let ok = unsafe {
        SendMessageTimeoutW(hwnd, WM_COPYDATA, 0, &cds as *const _ as isize, SMTO_ABORTIFHUNG, 3000, &mut result)
    };
    ok != 0 && result == 1
}
