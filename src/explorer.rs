//! Which folder File Explorer is showing: the window (or tab) in front, or the
//! one folder selected in it. Asked over COM, the same way Explorer's own
//! scripting interface works: nothing is injected, nothing keeps running.
//! The caller's thread must have COM initialised (the resident does at start).

use std::path::{Path, PathBuf};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, IServiceProvider};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, IFolderView2, IPersistFolder2, IShellBrowser, IShellItem, IShellWindows, KF_FLAG_DEFAULT,
    SHCreateItemFromIDList, SHGetKnownFolderPath, SID_STopLevelBrowser, SIGDN, SIGDN_FILESYSPATH, SIGDN_NORMALDISPLAY,
    ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GA_ROOT, GetAncestor, GetClassNameW, IsWindowVisible};
use windows::core::{Interface, w};

pub enum Found {
    Folder(PathBuf),
    /// Explorer is in front, but on something that isn't a folder on disk ("This PC", "Home").
    NotOnDisk(String),
    /// The window in front isn't File Explorer.
    NoExplorer,
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn name_of(item: &IShellItem, how: SIGDN) -> Option<String> {
    unsafe {
        let p = item.GetDisplayName(how).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

fn desktop() -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

/// The folder for the window `front` (a top-level window). With `selection`,
/// a single selected folder wins over the folder that's open.
pub fn folder_of(front: isize, selection: bool) -> Found {
    let front = HWND(front as *mut _);
    let class = class_of(front);
    if class == "Progman" || class == "WorkerW" {
        return desktop().map_or(Found::NoExplorer, Found::Folder);
    }
    if class != "CabinetWClass" && class != "ExploreWClass" {
        return Found::NoExplorer;
    }
    unsafe {
        let Ok(windows) = CoCreateInstance::<_, IShellWindows>(&ShellWindows, None, CLSCTX_ALL) else {
            return Found::NoExplorer;
        };
        // With tabs (Windows 11), every tab is a shell window in the same frame;
        // the tab on top is the frame's first ShellTabWindowClass child.
        let active_tab = FindWindowExW(Some(front), None, w!("ShellTabWindowClass"), None).ok();
        let mut candidates = Vec::new();
        for i in 0..windows.Count().unwrap_or(0) {
            let Ok(disp) = windows.Item(&VARIANT::from(i)) else { continue };
            let Ok(sp) = disp.cast::<IServiceProvider>() else { continue };
            let Ok(browser) = sp.QueryService::<IShellBrowser>(&SID_STopLevelBrowser) else { continue };
            let Ok(hwnd) = browser.GetWindow() else { continue };
            if GetAncestor(hwnd, GA_ROOT) != front {
                continue;
            }
            let rank = if Some(hwnd) == active_tab {
                0
            } else if IsWindowVisible(hwnd).as_bool() {
                1
            } else {
                2
            };
            candidates.push((rank, browser));
        }
        candidates.sort_by_key(|(rank, _)| *rank);
        let Some((_, browser)) = candidates.into_iter().next() else { return Found::NoExplorer };
        let Ok(view) = browser.QueryActiveShellView() else { return Found::NoExplorer };
        let Ok(view) = view.cast::<IFolderView2>() else { return Found::NoExplorer };

        if selection
            && let Ok(items) = view.GetSelection(false)
            && items.GetCount().unwrap_or(0) == 1
            && let Some(path) = items.GetItemAt(0).ok().and_then(|it| name_of(&it, SIGDN_FILESYSPATH))
            && Path::new(&path).is_dir()
        {
            return Found::Folder(PathBuf::from(path));
        }

        let Ok(folder) = view.GetFolder::<IPersistFolder2>() else { return Found::NoExplorer };
        let Ok(pidl) = folder.GetCurFolder() else { return Found::NoExplorer };
        let item = SHCreateItemFromIDList::<IShellItem>(pidl);
        CoTaskMemFree(Some(pidl as *const _));
        let Ok(item) = item else { return Found::NoExplorer };
        match name_of(&item, SIGDN_FILESYSPATH) {
            Some(path) if Path::new(&path).is_dir() => Found::Folder(PathBuf::from(path)),
            _ => Found::NotOnDisk(name_of(&item, SIGDN_NORMALDISPLAY).unwrap_or_else(|| "That".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

    /// tools/live-test/explorer-check.ps1 runs this: it asks about a real Explorer window (`TWIG_HWND`)
    /// without touching the keyboard or the focus.
    #[test]
    #[ignore]
    fn live_window() {
        let front: isize = std::env::var("TWIG_HWND").unwrap().parse().unwrap();
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        for selection in [false, true] {
            let said = match folder_of(front, selection) {
                Found::Folder(dir) => format!("folder {}", dir.display()),
                Found::NotOnDisk(name) => format!("not on disk: {name}"),
                Found::NoExplorer => "no explorer".to_string(),
            };
            println!("selection {selection}: {said}");
        }
    }
}
