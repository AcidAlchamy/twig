//! Text on the clipboard: plain Unicode text with Windows line endings, which
//! every program pastes. It stays there after Twig exits.

use std::time::Duration;
use windows_sys::Win32::Foundation::{GlobalFree, HWND};
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

/// Another program may have the clipboard open for a moment; try a few times.
fn open(owner: HWND) -> bool {
    for _ in 0..12 {
        if unsafe { OpenClipboard(owner) } != 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    false
}

pub fn put_text(owner: HWND, text: &str) -> bool {
    let crlf = text.replace("\r\n", "\n").replace('\n', "\r\n");
    let wide: Vec<u16> = crlf.encode_utf16().chain(Some(0)).collect();
    let bytes = wide.len() * 2;
    if !open(owner) {
        return false;
    }
    unsafe {
        EmptyClipboard();
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes);
        let mut ok = false;
        if !h.is_null() {
            let p = GlobalLock(h);
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), p.cast::<u8>(), bytes);
                GlobalUnlock(h);
                // On success the clipboard owns the memory.
                ok = !SetClipboardData(CF_UNICODETEXT as u32, h).is_null();
            }
            if !ok {
                GlobalFree(h);
            }
        }
        CloseClipboard();
        ok
    }
}
