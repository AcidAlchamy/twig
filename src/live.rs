//! A few bytes of shared memory between the resident and the settings window:
//! how many trees so far, and what became of the hotkey. Plain atomics, no
//! locks, no messages.

use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Memory::{CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, PAGE_READWRITE};

#[repr(C)]
pub struct Live {
    copies: AtomicU32,
    hotkey: AtomicU32,
}

/// What became of the hotkey.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyState {
    Off,
    Ready,
    /// Another program already owns that key combination.
    Taken,
    /// The text couldn't be understood.
    Invalid,
    /// Paused from the tray.
    Paused,
}

impl Live {
    pub fn add_copy(&self) {
        self.copies.fetch_add(1, Relaxed);
    }

    pub fn copies(&self) -> u32 {
        self.copies.load(Relaxed)
    }

    pub fn set_hotkey(&self, s: HotkeyState) {
        self.hotkey.store(s as u32, Relaxed);
    }

    pub fn hotkey(&self) -> HotkeyState {
        match self.hotkey.load(Relaxed) {
            1 => HotkeyState::Ready,
            2 => HotkeyState::Taken,
            3 => HotkeyState::Invalid,
            4 => HotkeyState::Paused,
            _ => HotkeyState::Off,
        }
    }
}

/// Opens (or creates) the shared block. Lives for the rest of the process.
pub fn open() -> Option<&'static Live> {
    let name = crate::win::wide("Local\\Twig.Live.v1");
    unsafe {
        let map = CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            std::ptr::null(),
            PAGE_READWRITE,
            0,
            size_of::<Live>() as u32,
            name.as_ptr(),
        );
        if map.is_null() {
            return None;
        }
        let view = MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, size_of::<Live>());
        // A fresh mapping is zeroed, which is a valid `Live`.
        (view.Value as *const Live).as_ref()
    }
}
