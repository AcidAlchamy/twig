//! Twig: right-click a folder, get its tree. Copied, ready to paste into a
//! chat, a README or an issue, without node_modules or anything .gitignore
//! leaves out.
//!
//! One small exe with five faces:
//!   Twig.exe                the resident: tray icon and the Explorer hotkey
//!                           (plain Win32, asleep until you press it)
//!   Twig.exe --copy <dir>   copies a folder's tree: what the right-click menu
//!                           runs (the resident does it if it's running)
//!   Twig.exe --settings     the settings window (only alive while it's open)
//!   Twig.exe --hello        the donationware hello card shown at start
//!   Twig.exe --uninstall    takes Twig out of the right-click menu and start-up
//!
//! Twig is donationware by Acid Alchamy: free to use, and if it helps, please
//! consider a donation at acidalchamy.com/donationware.

#![windows_subsystem = "windows"]

mod autostart;
mod clip;
mod config;
mod explorer;
mod hotkey;
mod ipc;
mod live;
mod paint;
mod resident;
mod shellmenu;
mod tree;
mod ui;
mod win;

use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DONATE_URL: &str = "https://acidalchamy.com/donationware/twig/?from=app#donate";
pub const HOME_URL: &str = "https://acidalchamy.com/donationware/twig/";
pub const REPO_URL: &str = "https://github.com/AcidAlchamy/twig";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    if has("--settings") {
        ui::run_settings(has("--welcome"));
    } else if has("--hello") {
        ui::run_hello();
    } else if let Some(dir) = value("--copy") {
        let dir = folder_arg(&dir);
        if !ipc::request_copy(&dir) {
            resident::run_once(PathBuf::from(dir));
        }
    } else if has("--uninstall") {
        uninstall();
    } else {
        resident::run(has("--autostart"), has("--no-hello"));
    }
}

/// Explorer passes a drive as "C:\", and in `"C:\"` the backslash escapes the
/// closing quote, so the argument arrives as `C:"`. Put it back.
fn folder_arg(arg: &str) -> String {
    let a = arg.trim().trim_end_matches('"');
    if a.len() == 2 && a.ends_with(':') { format!("{a}\\") } else { a.to_owned() }
}

/// Out of the right-click menu and start-up, settings gone, the resident quit.
/// What's left is Twig.exe itself, which can then simply be deleted.
fn uninstall() {
    shellmenu::remove();
    autostart::set(false);
    ipc::send(ipc::WM_TWIG_QUIT, 0, 0);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let _ = std::fs::remove_dir_all(config::dir());
    win::message(
        "Twig",
        "Twig is out of the right-click menu and won't start with Windows. Its settings are gone too.\n\nYou can delete Twig.exe now.",
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn drive_arguments() {
        assert_eq!(super::folder_arg("C:\""), "C:\\");
        assert_eq!(super::folder_arg(r"D:\Code\my-app"), r"D:\Code\my-app");
        assert_eq!(super::folder_arg("E:"), "E:\\");
    }
}
