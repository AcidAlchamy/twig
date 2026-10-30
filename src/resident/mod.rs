//! The resident: a hidden window, a tray icon and the Explorer hotkey.
//!
//! It sleeps in GetMessage until you press the hotkey or the right-click menu
//! hands it a folder. Nothing polls; a timer runs only while the card is on
//! screen. Folders are walked on a short-lived thread, so a big one never
//! freezes anything. `Twig.exe --copy` with no resident running runs this same
//! code once, without the tray or the hotkey, and exits when its card is gone.

mod card;
mod tray;

use crate::explorer::{self, Found};
use crate::ipc::{COPY_TAG, RESIDENT_CLASS, WM_TWIG_QUIT, WM_TWIG_RELOAD, WM_TWIG_SETTINGS};
use crate::live::{HotkeyState, Live};
use crate::tree::{self, Built, Style};
use crate::win::{open_path, open_url, spawn_self, wide};
use crate::{autostart, clip, config, hotkey, live, shellmenu};
use card::{Hit, Info, Status};
use config::Config;
use std::cell::RefCell;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};
use tray::{MenuInfo, Pick, Tray, WM_TRAY};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, POINT};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ChangeWindowMessageFilterEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
    GetMessageW, KillTimer, MSG, MSGFLT_ALLOW, PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW,
    SetTimer, TranslateMessage, WM_APP, WM_CONTEXTMENU, WM_COPYDATA, WM_DESTROY, WM_HOTKEY, WM_TIMER, WM_USER,
    WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
};

const TIMER_FRAME: usize = 1;
const HOTKEY_ID: i32 = 1;
/// A walk finished.
const WM_TWIG_BUILT: u32 = WM_APP + 30;
/// How many lines of the tree the card shows.
const PREVIEW_LINES: usize = 8;

const NIN_SELECT: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;

/// A finished walk, handed back from its thread.
struct Done {
    id: u32,
    folder: String,
    result: Result<Built, String>,
}

struct App {
    hwnd: HWND,
    cfg: Config,
    live: Option<&'static Live>,
    /// None when running once for the right-click menu.
    tray: Option<Tray>,
    card: card::Card,
    /// The tree on the clipboard, for "copy it again as Markdown".
    last: Option<(Built, Style)>,
    done: Arc<Mutex<Vec<Done>>>,
    next_id: u32,
    busy: Option<u32>,
    frame_ms: u32,
    taskbar_created: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the app unless it's already borrowed further up the stack
/// (a modal loop like the tray menu re-enters the window procedure; what
/// arrives then is simply ignored).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

pub(crate) fn card_mouse(msg: u32, x: i32, y: i32) {
    with_app(|app| {
        if let Some(hit) = app.card.mouse(msg, x, y) {
            app.on_card(hit);
        }
    });
}

pub(crate) fn card_wants_hand() -> bool {
    with_app(|app| app.card.wants_hand()).unwrap_or(false)
}

fn start_com() {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
}

pub fn run(autostarted: bool, no_hello: bool) {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let name = wide("Local\\Twig.Resident");
    let _mutex = unsafe { CreateMutexW(null(), 1, name.as_ptr()) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // Already running: a second launch opens the settings instead.
        if !autostarted {
            crate::ipc::send(WM_TWIG_SETTINGS, 0, 0);
        }
        return;
    }
    start_com();

    let first_run = !config::exists();
    let cfg = Config::load();
    if first_run {
        let _ = cfg.save();
        shellmenu::install();
    }
    autostart::follow_moves();
    shellmenu::follow_moves();

    let hwnd = create_window(RESIDENT_CLASS);
    if hwnd.is_null() {
        return;
    }
    for msg in [WM_TWIG_RELOAD, WM_TWIG_SETTINGS, WM_TWIG_QUIT, WM_COPYDATA] {
        unsafe { ChangeWindowMessageFilterEx(hwnd, msg, MSGFLT_ALLOW, null_mut()) };
    }

    let tray = Tray::new(hwnd, &tip(&cfg));
    let mut app = App::new(hwnd, cfg, Some(tray));
    app.apply();
    let hello = app.cfg.hello_card && !no_hello;
    APP.with(|cell| *cell.borrow_mut() = Some(app));

    if first_run {
        spawn_self(&["--settings", "--welcome"]);
    } else if hello {
        spawn_self(&["--hello"]);
    }
    message_loop();
}

/// The right-click menu's copy when no resident is running: copy, show the
/// card, and exit once it's gone.
pub fn run_once(root: PathBuf) {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    start_com();
    let hwnd = create_window("Twig.Once");
    if hwnd.is_null() {
        return;
    }
    let mut app = App::new(hwnd, Config::load(), None);
    app.copy(root);
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    message_loop();
}

fn message_loop() {
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    APP.with(|cell| cell.borrow_mut().take());
}

/// The tray icon's tooltip: what the hotkey does.
fn tip(cfg: &Config) -> String {
    if cfg.hotkey.trim().is_empty() {
        "Twig: right-click a folder, Copy tree with Twig".into()
    } else {
        format!("Twig: {} copies the tree of the folder in Explorer", cfg.hotkey)
    }
}

fn create_window(class_name: &str) -> HWND {
    let hinst = unsafe { GetModuleHandleW(null()) };
    let class = wide(class_name);
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    unsafe {
        RegisterClassExW(&wc);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("Twig").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        )
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_HOTKEY if wparam == HOTKEY_ID as usize => {
            with_app(|app| app.on_hotkey());
        }
        WM_TIMER if wparam == TIMER_FRAME => {
            with_app(|app| app.on_frame());
        }
        WM_COPYDATA => {
            let cds = unsafe { &*(lparam as *const COPYDATASTRUCT) };
            if cds.dwData != COPY_TAG || cds.lpData.is_null() {
                return 0;
            }
            let units = unsafe { std::slice::from_raw_parts(cds.lpData as *const u16, cds.cbData as usize / 2) };
            let path = PathBuf::from(String::from_utf16_lossy(units));
            // 1 = taken. 0 (busy in a menu) makes the caller copy it itself.
            return with_app(|app| app.copy(path)).map_or(0, |()| 1);
        }
        WM_TRAY => {
            let event = (lparam & 0xFFFF) as u32;
            match event {
                WM_CONTEXTMENU => {
                    let at = POINT { x: (wparam & 0xFFFF) as i16 as i32, y: ((wparam >> 16) & 0xFFFF) as i16 as i32 };
                    // Read what the menu needs, then let go of the app: the menu
                    // (and the folder picker) run their own message loops.
                    if let Some((Some(tray), info)) = with_app(|app| (app.tray, app.menu_info())) {
                        match tray.menu(at, &info) {
                            Some(Pick::Browse) => {
                                if let Some(dir) = pick_folder() {
                                    with_app(|app| app.copy(dir));
                                }
                            }
                            Some(pick) => {
                                with_app(|app| app.on_pick(pick));
                            }
                            None => {}
                        }
                    }
                }
                NIN_SELECT | NIN_KEYSELECT => {
                    with_app(|app| app.open_settings());
                }
                _ => {}
            }
        }
        WM_TWIG_BUILT => {
            with_app(|app| app.on_built());
        }
        WM_TWIG_RELOAD => {
            with_app(|app| app.reload());
        }
        WM_TWIG_SETTINGS => {
            with_app(|app| app.open_settings());
        }
        WM_TWIG_QUIT => {
            with_app(|app| app.quit());
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
        }
        _ => {
            if with_app(|app| msg == app.taskbar_created && msg != 0).unwrap_or(false) {
                with_app(|app| {
                    if let Some(t) = app.tray {
                        t.add(app.cfg.paused, &tip(&app.cfg));
                    }
                });
            }
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn pick_folder() -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title("Copy the tree of which folder?");
    if let Some(dir) = config::last_folder() {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_folder()
}

impl App {
    fn new(hwnd: HWND, cfg: Config, tray: Option<Tray>) -> Self {
        Self {
            hwnd,
            cfg,
            live: live::open(),
            tray,
            card: card::Card::new(),
            last: None,
            done: Arc::new(Mutex::new(Vec::new())),
            next_id: 1,
            busy: None,
            frame_ms: 0,
            taskbar_created: unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) },
        }
    }

    fn once(&self) -> bool {
        self.tray.is_none()
    }

    fn apply(&mut self) {
        unsafe { UnregisterHotKey(self.hwnd, HOTKEY_ID) };
        let key = self.cfg.hotkey.trim();
        let state = if key.is_empty() {
            HotkeyState::Off
        } else if self.cfg.paused {
            HotkeyState::Paused
        } else {
            match hotkey::parse(key) {
                Some((mods, vk)) if unsafe { RegisterHotKey(self.hwnd, HOTKEY_ID, mods, vk) } != 0 => {
                    HotkeyState::Ready
                }
                Some(_) => HotkeyState::Taken,
                None => HotkeyState::Invalid,
            }
        };
        if let Some(live) = self.live {
            live.set_hotkey(state);
        }
        if let Some(t) = self.tray {
            t.update(self.cfg.paused, &tip(&self.cfg));
        }
    }

    fn reload(&mut self) {
        self.cfg = Config::load();
        self.apply();
    }

    fn menu_info(&self) -> MenuInfo {
        MenuInfo {
            last: config::last_folder()
                .map(|p| p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())),
            paused: self.cfg.paused,
            hotkey: self.cfg.hotkey.clone(),
        }
    }

    fn on_pick(&mut self, pick: Pick) {
        match pick {
            Pick::Browse => {}
            Pick::Again => {
                if let Some(dir) = config::last_folder() {
                    self.copy(dir);
                }
            }
            Pick::Pause => {
                self.cfg.paused = !self.cfg.paused;
                let _ = self.cfg.save();
                self.apply();
            }
            Pick::Settings => self.open_settings(),
            Pick::Donate => open_url(crate::DONATE_URL),
            Pick::Website => open_url(crate::HOME_URL),
            Pick::Quit => self.quit(),
        }
    }

    /// Opening the settings also picks up hand edits to settings.toml.
    fn open_settings(&mut self) {
        self.reload();
        spawn_self(&["--settings"]);
    }

    fn quit(&mut self) {
        self.card.hide();
        if let Some(t) = self.tray {
            t.remove();
        }
        unsafe { DestroyWindow(self.hwnd) };
    }

    fn take_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    fn on_hotkey(&mut self) {
        let front = unsafe { GetForegroundWindow() };
        match explorer::folder_of(front as isize, self.cfg.use_selection) {
            Found::Folder(dir) => self.copy(dir),
            Found::NotOnDisk(name) => self.note("Not a folder on disk", &format!("\u{201C}{name}\u{201D} has no tree to copy.")),
            Found::NoExplorer => self.note("No folder in front", "Open a folder in File Explorer first."),
        }
    }

    fn note(&mut self, title: &str, line: &str) {
        let id = self.take_id();
        self.card.show(Info { id, ..Info::default() }, Status::Note(title.into(), line.into()));
        self.ensure_timer();
    }

    /// Walks `root` on a thread; `on_built` copies the result.
    fn copy(&mut self, root: PathBuf) {
        let id = self.take_id();
        self.busy = Some(id);
        let folder = root.file_name().map_or_else(|| root.display().to_string(), |n| n.to_string_lossy().into_owned());
        if self.cfg.card {
            self.card.show(Info { id, folder: folder.clone(), ..Info::default() }, Status::Reading);
        }
        let (opts, done, hwnd) = (self.cfg.options(), self.done.clone(), self.hwnd as isize);
        std::thread::spawn(move || {
            let result = tree::build(&root, &opts).map_err(|e| friendly(&e));
            if let Ok(mut list) = done.lock() {
                list.push(Done { id, folder, result });
            }
            unsafe { PostMessageW(hwnd as HWND, WM_TWIG_BUILT, 0, 0) };
        });
        self.ensure_timer();
    }

    fn on_built(&mut self) {
        let items = self.done.lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default();
        for item in items {
            if self.busy != Some(item.id) {
                continue; // a newer copy was asked for since
            }
            self.busy = None;
            match item.result {
                Ok(built) => {
                    let style = self.cfg.style;
                    self.deliver(built, style, item.id);
                }
                Err(why) => {
                    let info = Info { id: item.id, folder: item.folder, ..Info::default() };
                    self.card.update(info, Status::Failed(why));
                }
            }
        }
        self.ensure_timer();
        self.maybe_exit();
    }

    /// Puts `built` on the clipboard in `style` and says so on the card.
    fn deliver(&mut self, built: Built, style: Style, id: u32) {
        let opts = self.cfg.options();
        let out = built.render(style, &opts);
        let copied = clip::put_text(self.hwnd, &out.text);
        if copied {
            if let Some(live) = self.live {
                live.add_copy();
            }
            config::set_last_folder(&built.root);
        }
        if self.cfg.card {
            let peek = tree::Options { max_lines: PREVIEW_LINES, ..opts.clone() };
            let preview = built.render(Style::Tree, &peek).text.lines().take(PREVIEW_LINES).map(String::from).collect();
            let info = Info {
                id,
                folder: built.name().to_owned(),
                style: Some(style),
                preview,
                lines: out.lines,
                folders: built.folders,
                files: built.files,
                skipped: built.skipped.clone(),
                gitignore: opts.gitignore,
            };
            let status = if copied { Status::Copied } else { Status::Failed("Couldn't reach the clipboard".into()) };
            if self.card.active() && self.card.id() == id {
                self.card.update(info, status);
            } else {
                self.card.show(info, status);
            }
        }
        self.last = Some((built, style));
    }

    fn on_card(&mut self, hit: Hit) {
        match hit {
            Hit::Again(style) => {
                if let Some((built, _)) = self.last.clone() {
                    let id = self.card.id();
                    self.deliver(built, style, id);
                }
            }
            Hit::View => {
                if let Some((built, style)) = &self.last {
                    let text = built.render(*style, &self.cfg.options()).text.replace('\n', "\r\n");
                    let dir = std::env::temp_dir().join("Twig");
                    let file = dir.join(format!(
                        "{} tree.{}",
                        built.name(),
                        if *style == Style::Markdown { "md" } else { "txt" }
                    ));
                    if std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&file, text).is_ok() {
                        open_path(&file);
                    }
                }
                self.card.dismiss();
            }
            Hit::Close => self.card.dismiss(),
            Hit::Nothing => {}
        }
        self.ensure_timer();
    }

    fn ensure_timer(&mut self) {
        let want = if self.card.animating() {
            16
        } else if self.card.active() {
            125
        } else {
            0
        };
        if want != self.frame_ms {
            unsafe {
                if want == 0 {
                    KillTimer(self.hwnd, TIMER_FRAME);
                } else {
                    SetTimer(self.hwnd, TIMER_FRAME, want, None);
                }
            }
            self.frame_ms = want;
        }
    }

    fn on_frame(&mut self) {
        self.card.tick();
        self.ensure_timer();
        self.maybe_exit();
    }

    /// Running once for the right-click menu: done when nothing is left to show.
    fn maybe_exit(&mut self) {
        if self.once() && self.busy.is_none() && !self.card.active() {
            unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

/// A read error in a few plain words.
fn friendly(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        PermissionDenied => "No permission to read that folder".into(),
        NotFound => "That folder isn't there any more".into(),
        _ => {
            let s = e.to_string();
            let s =
                if s.chars().count() > 60 { format!("{}\u{2026}", s.chars().take(59).collect::<String>()) } else { s };
            let mut c = s.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    }
}
