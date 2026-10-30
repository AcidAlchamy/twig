//! The settings window. Every change saves on its own (a quarter second after
//! you stop typing) and the resident picks it up straight away. The preview
//! shows a real folder's tree as it would be copied with these settings.

use super::theme::{self, ACID, BAD, BG, FAINT, LINE, MUTED, TEXT, VIOLET};
use super::widgets::{self, card, row, toggle};
use crate::config::{self, Config};
use crate::ipc::{self, WM_TWIG_QUIT, WM_TWIG_RELOAD};
use crate::live::{HotkeyState, Live};
use crate::tree::{self, Style};
use crate::win;
use crate::{autostart, hotkey, shellmenu};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, ScrollArea, Sense, Stroke, Ui, pos2,
    vec2,
};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, SystemTime};

const SAVE_DELAY: f64 = 0.25;

/// What the preview shows and how it was made.
struct Preview {
    folder: Option<PathBuf>,
    made_with: Option<(PathBuf, tree::Options, Style)>,
    text: String,
    stats: String,
    failed: bool,
    pending: Option<Receiver<Made>>,
}

struct Made {
    key: (PathBuf, tree::Options, Style),
    result: Result<(String, String), String>,
}

pub struct Settings {
    cfg: Config,
    saved: Config,
    changed_at: Option<f64>,
    file_stamp: Option<SystemTime>,
    welcome: bool,
    autostart: bool,
    menu: bool,
    live: Option<&'static Live>,
    recording: bool,
    resident_up: bool,
    last_poll: f64,
    last_copies: u32,
    copy_flash: f64,
    note: Option<(String, Color32, f64)>,
    skip_text: String,
    uninstall_armed: Option<f64>,
    preview: Preview,
}

fn file_stamp() -> Option<SystemTime> {
    std::fs::metadata(config::path()).and_then(|m| m.modified()).ok()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Settings {
    pub fn new(welcome: bool) -> Self {
        let cfg = Config::load();
        let live = crate::live::open();
        Self {
            skip_text: cfg.skip.join(", "),
            saved: cfg.clone(),
            cfg,
            changed_at: None,
            file_stamp: file_stamp(),
            welcome,
            autostart: autostart::is_enabled(),
            menu: shellmenu::is_installed(),
            live,
            recording: false,
            resident_up: ipc::resident().is_some(),
            last_poll: 0.0,
            last_copies: live.map_or(0, |l| l.copies()),
            copy_flash: -10.0,
            note: None,
            uninstall_armed: None,
            preview: Preview {
                folder: config::last_folder(),
                made_with: None,
                text: String::new(),
                stats: String::new(),
                failed: false,
                pending: None,
            },
        }
    }

    fn save_now(&mut self) {
        if self.cfg != self.saved && self.cfg.save().is_ok() {
            self.saved = self.cfg.clone();
            self.file_stamp = file_stamp();
            ipc::send(WM_TWIG_RELOAD, 0, 0);
        }
        self.changed_at = None;
    }

    fn note(&mut self, text: impl Into<String>, color: Color32, now: f64) {
        self.note = Some((text.into(), color, now + 4.0));
    }

    /// Once a second: is the resident there, did someone else change the file, any new copies.
    fn poll(&mut self, now: f64) {
        if let Some(live) = self.live {
            let n = live.copies();
            if n != self.last_copies {
                self.last_copies = n;
                self.copy_flash = now;
                // A copy just happened: show that folder.
                if let Some(dir) = config::last_folder() {
                    self.preview.folder = Some(dir);
                }
            }
        }
        if now - self.last_poll < 1.0 {
            return;
        }
        self.last_poll = now;
        self.resident_up = ipc::resident().is_some();
        self.menu = shellmenu::is_installed();
        let stamp = file_stamp();
        if stamp != self.file_stamp && self.changed_at.is_none() {
            self.file_stamp = stamp;
            self.cfg = Config::load();
            self.saved = self.cfg.clone();
            self.skip_text = self.cfg.skip.join(", ");
        }
    }

    fn record_hotkey(&mut self, ui: &Ui, now: f64) {
        if !self.recording {
            return;
        }
        let events = ui.input(|i| i.events.clone());
        for e in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            match key {
                egui::Key::Escape => self.recording = false,
                egui::Key::Backspace | egui::Key::Delete => {
                    self.cfg.hotkey.clear();
                    self.changed_at = Some(now);
                    self.recording = false;
                }
                _ => {
                    let mut parts = vec![];
                    if modifiers.ctrl {
                        parts.push("Ctrl");
                    }
                    if modifiers.alt {
                        parts.push("Alt");
                    }
                    if modifiers.shift {
                        parts.push("Shift");
                    }
                    parts.push(key.name());
                    let combo = parts.join("+");
                    if hotkey::parse(&combo).is_some() {
                        self.cfg.hotkey = combo;
                        self.changed_at = Some(now);
                        self.recording = false;
                    } else {
                        self.note("Hold Ctrl or Alt with the key (or use F1 to F24).", BAD, now);
                    }
                }
            }
        }
    }

    /// Rebuilds the preview on a thread when the folder or a setting changed.
    fn refresh_preview(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.preview.pending {
            match rx.try_recv() {
                Ok(made) => {
                    self.preview.pending = None;
                    self.preview.failed = made.result.is_err();
                    (self.preview.text, self.preview.stats) = made.result.unwrap_or_else(|e| (String::new(), e));
                    self.preview.made_with = Some(made.key);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(_) => self.preview.pending = None,
            }
        }
        let Some(folder) = self.preview.folder.clone() else { return };
        let key = (folder.clone(), self.cfg.options(), self.cfg.style);
        if self.preview.made_with.as_ref() == Some(&key) {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let (_, opts, style) = &key;
            let result = tree::build(&folder, opts).map(|b| {
                let out = b.render(*style, opts);
                let mut stats = format!(
                    "{}  \u{00B7}  {}  \u{00B7}  {}",
                    plural(out.lines, "line", "lines"),
                    plural(b.folders, "folder", "folders"),
                    plural(b.files, "file", "files")
                );
                if !b.skipped.is_empty() {
                    stats.push_str(&format!("  \u{00B7}  left out {}", b.skipped.join(", ")));
                }
                (out.text, stats)
            });
            let _ = tx.send(Made { key, result: result.map_err(|e| e.to_string()) });
            ctx.request_repaint();
        });
        self.preview.pending = Some(rx);
    }
}

impl eframe::App for Settings {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let now = ui.input(|i| i.time);
        self.poll(now);
        self.record_hotkey(ui, now);
        self.refresh_preview(ui.ctx());
        if self.uninstall_armed.is_some_and(|t| now > t) {
            self.uninstall_armed = None;
        }
        if ui.input(|i| i.viewport().close_requested()) {
            self.save_now();
        }

        egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ui, |ui| {
            self.header(ui, now);
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                Frame::new().inner_margin(Margin { left: 20, right: 20, top: 14, bottom: 18 }).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    if self.welcome {
                        self.welcome_card(ui);
                    }
                    self.status_row(ui, now);
                    self.copy_card(ui, now);
                    self.preview_card(ui, now);
                    self.content_card(ui, now);
                    self.style_card(ui, now);
                    self.behaviour_card(ui, now);
                    self.footer(ui);
                });
            });
        });

        if let Some((text, color, until)) = self.note.clone() {
            if now < until {
                egui::Area::new(egui::Id::unique("twig-note"))
                    .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -18.0))
                    .show(ui.ctx(), |ui| {
                        Frame::new()
                            .fill(Color32::from_rgb(0x1C, 0x17, 0x29))
                            .stroke(Stroke::new(1.0, color))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(14, 9))
                            .show(ui, |ui| ui.label(RichText::new(text).color(TEXT)));
                    });
            } else {
                self.note = None;
            }
        }

        if let Some(t) = self.changed_at.or((self.cfg != self.saved).then_some(now)) {
            self.changed_at = Some(t);
            if now - t >= SAVE_DELAY {
                self.save_now();
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(60));
            }
        }

        let busy = self.recording || now - self.copy_flash < 1.0 || self.preview.pending.is_some();
        ui.ctx().request_repaint_after(Duration::from_millis(if busy { 16 } else { 250 }));
    }
}

impl Settings {
    fn header(&mut self, ui: &mut Ui, now: f64) {
        let height = 92.0;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let painter = ui.painter_at(rect);

        let since = (now - self.copy_flash) as f32;
        let pulse = if since < 0.5 { 1.0 - (since - 0.25).abs() / 0.25 } else { 0.0 };
        let logo = Rect::from_center_size(rect.left_center() + vec2(58.0, 0.0), vec2(58.0, 58.0));
        widgets::logo(&painter, logo, pulse.max(0.0));

        let x = logo.right() + 16.0;
        painter.text(pos2(x, rect.center().y - 14.0), egui::Align2::LEFT_CENTER, "Twig", theme::bold(28.0), TEXT);
        painter.text(
            pos2(x + 1.0, rect.center().y + 15.0),
            egui::Align2::LEFT_CENTER,
            format!("v{}  \u{00B7}  donationware by Acid Alchamy", crate::VERSION),
            egui::FontId::proportional(13.0),
            MUTED,
        );
        widgets::brand_rule(&painter, Rect::from_min_max(pos2(rect.left(), rect.bottom() - 2.0), rect.right_bottom()));

        let button = Rect::from_min_size(pos2(rect.right() - 20.0 - 118.0, rect.center().y - 17.0), vec2(118.0, 34.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(button));
        if widgets::donate(&mut child, "Donate").clicked() {
            win::open_url(crate::DONATE_URL);
        }
    }

    fn welcome_card(&mut self, ui: &mut Ui) {
        let key = self.cfg.hotkey.trim().to_owned();
        let hotkey = if key.is_empty() { String::new() } else { format!(" Or open a folder and press {key}.") };
        Frame::new()
            .fill(Color32::from_rgb(0x1A, 0x12, 0x2C))
            .stroke(Stroke::new(1.0, VIOLET.gamma_multiply(0.7)))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Welcome to Twig").font(theme::bold(18.0)).color(TEXT));
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "Right-click any folder in File Explorer and choose \u{201C}{}\u{201D} (on Windows 11 it's \
                             under Show more options, or hold Shift as you right-click).{hotkey} The tree lands on your \
                             clipboard, without node_modules or anything .gitignore leaves out. Paste it into a chat, \
                             a README or an issue. Everything below is optional.",
                            shellmenu::LABEL
                        ))
                        .color(MUTED),
                    )
                    .wrap(),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "Twig is donationware: free to use, for good. If it saves you some typing, a small \
                             donation keeps the lab building more little tools like it.",
                        )
                        .color(TEXT),
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::donate(ui, "Donate").clicked() {
                        win::open_url(crate::DONATE_URL);
                    }
                    if widgets::ghost(ui, "Got it").clicked() {
                        self.welcome = false;
                    }
                });
            });
    }

    fn status_row(&mut self, ui: &mut Ui, now: f64) {
        ui.horizontal(|ui| {
            if !self.resident_up {
                let text = if self.menu {
                    "Twig isn't running (the right-click menu still works)"
                } else {
                    "Twig isn't running"
                };
                widgets::status(ui, BAD, text);
            } else if self.cfg.paused {
                widgets::status(
                    ui,
                    Color32::from_rgb(0xFF, 0xC4, 0x3A),
                    "Hotkey paused (the right-click menu still works)",
                );
            } else {
                let text = match self.last_copies {
                    0 => "Ready. Right-click a folder to copy its tree.".to_owned(),
                    1 => "Ready. One tree copied so far.".to_owned(),
                    n => format!("Ready. {n} trees copied so far."),
                };
                let flash = (1.0 - (now - self.copy_flash) as f32).max(0.0);
                widgets::status(ui, theme::lerp(ACID, Color32::WHITE, flash), &text);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.resident_up {
                    if widgets::primary(ui, "Start Twig").clicked() {
                        self.save_now();
                        win::spawn_self(&[]);
                        self.last_poll = 0.0;
                    }
                } else if !self.cfg.hotkey.trim().is_empty() {
                    let mut on = !self.cfg.paused;
                    if toggle(ui, &mut on).changed() {
                        self.cfg.paused = !on;
                        self.changed_at = Some(now - SAVE_DELAY);
                    }
                    ui.label(RichText::new("Hotkey").color(MUTED));
                }
            });
        });
    }

    fn copy_card(&mut self, ui: &mut Ui, now: f64) {
        let state = self.live.map_or(HotkeyState::Off, |l| l.hotkey());
        card(ui, "Copy a tree", |ui| {
            let mut menu = self.menu;
            row(
                ui,
                "In the right-click menu",
                "\u{201C}Copy tree with Twig\u{201D} on folders and drives. Windows 11: Show more options",
                |ui| {
                    if toggle(ui, &mut menu).changed() {
                        if menu {
                            shellmenu::install()
                        } else {
                            shellmenu::remove()
                        }
                        self.menu = shellmenu::is_installed();
                    }
                },
            );
            row(ui, "Hotkey in File Explorer", "Copies the folder that's open in front", |ui| {
                if !self.cfg.hotkey.is_empty() && !self.recording && widgets::ghost(ui, "Clear").clicked() {
                    self.cfg.hotkey.clear();
                    self.changed_at = Some(now);
                }
                if widgets::ghost(ui, if self.recording { "Cancel" } else { "Set" }).clicked() {
                    self.recording = !self.recording;
                }
                if self.recording {
                    ui.label(RichText::new("Press the keys\u{2026}").color(ACID));
                } else if self.cfg.hotkey.is_empty() {
                    ui.label(RichText::new("None").color(FAINT));
                } else {
                    match state {
                        HotkeyState::Taken => {
                            ui.label(RichText::new("taken by another app").small().color(BAD));
                        }
                        HotkeyState::Invalid => {
                            ui.label(RichText::new("not a usable key").small().color(BAD));
                        }
                        _ => {}
                    }
                    ui.horizontal(|ui| widgets::keycaps(ui, &self.cfg.hotkey));
                }
            });
            let mut sel = self.cfg.use_selection;
            row(ui, "Use the selected folder", "With one folder selected, the hotkey copies that one", |ui| {
                if toggle(ui, &mut sel).changed() {
                    self.cfg.use_selection = sel;
                    self.changed_at = Some(now);
                }
            });
        });
    }

    fn preview_card(&mut self, ui: &mut Ui, now: f64) {
        card(ui, "Preview", |ui| {
            ui.horizontal(|ui| {
                let name = self.preview.folder.as_ref().map(|p| p.display().to_string());
                match &name {
                    Some(n) => {
                        ui.add(egui::Label::new(RichText::new(n).color(TEXT)).truncate()).on_hover_text(n);
                    }
                    None => {
                        ui.label(RichText::new("Pick a folder to see its tree here.").color(MUTED));
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ready = !self.preview.text.is_empty() && !self.preview.failed;
                    if ui.add_enabled(ready, egui::Button::new("Copy")).clicked() {
                        ui.ctx().copy_text(self.preview.text.clone());
                        self.note("Copied. Paste it anywhere.", ACID, now);
                    }
                    if widgets::ghost(ui, "Pick a folder\u{2026}").clicked() {
                        let mut dialog = rfd::FileDialog::new().set_title("Preview which folder?");
                        if let Some(dir) = &self.preview.folder {
                            dialog = dialog.set_directory(dir);
                        }
                        if let Some(dir) = dialog.pick_folder() {
                            self.preview.folder = Some(dir);
                        }
                    }
                });
            });
            if self.preview.folder.is_none() {
                return;
            }
            let status = if self.preview.pending.is_some() && self.preview.text.is_empty() {
                ("Reading\u{2026}".to_owned(), FAINT)
            } else if self.preview.failed {
                (format!("Couldn't read it: {}", self.preview.stats), BAD)
            } else {
                (self.preview.stats.clone(), FAINT)
            };
            ui.label(RichText::new(status.0).small().color(status.1));
            Frame::new()
                .fill(Color32::from_rgb(0x0A, 0x08, 0x10))
                .stroke(Stroke::new(1.0, LINE))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ScrollArea::both().id_salt("preview").max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&self.preview.text).font(egui::FontId::monospace(13.0)).color(TEXT),
                            )
                            .extend(),
                        );
                    });
                });
        });
    }

    fn content_card(&mut self, ui: &mut Ui, now: f64) {
        card(ui, "What goes in", |ui| {
            let mut changed = false;
            let flag = |ui: &mut Ui, value: &mut bool, label: &str, hint: &str| {
                let mut c = false;
                row(ui, label, hint, |ui| c = toggle(ui, value).changed());
                c
            };
            changed |= flag(
                ui,
                &mut self.cfg.gitignore,
                "Follow .gitignore",
                "Leave out what .gitignore, .ignore and Git's own excludes list",
            );
            changed |= flag(ui, &mut self.cfg.hidden, "Dot and hidden files", ".github, .env.example and friends");
            changed |= flag(ui, &mut self.cfg.folders_only, "Folders only", "Just the shape, no files");
            changed |= flag(ui, &mut self.cfg.sizes, "File sizes", "After each file: main.rs  (4.2 KB)");
            row(ui, "How deep", "Levels below the folder", |ui| {
                let shown = if self.cfg.depth == 0 { "All the way".to_owned() } else { self.cfg.depth.to_string() };
                ui.label(RichText::new(shown).font(theme::semibold(14.0)).color(ACID));
                changed |= ui.add(egui::Slider::new(&mut self.cfg.depth, 0..=12).show_value(false)).changed();
            });
            row(ui, "Long folders", "Show this many, then \u{201C}\u{2026} 312 more\u{201D}", |ui| {
                let shown =
                    if self.cfg.per_folder == 0 { "No limit".to_owned() } else { self.cfg.per_folder.to_string() };
                ui.label(RichText::new(shown).font(theme::semibold(14.0)).color(ACID));
                changed |= ui
                    .add(egui::Slider::new(&mut self.cfg.per_folder, 0..=200).step_by(5.0).show_value(false))
                    .changed();
            });
            ui.label(RichText::new("Always leave out").color(TEXT));
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.skip_text)
                    .font(egui::FontId::monospace(13.5))
                    .desired_width(f32::INFINITY)
                    .hint_text(".git, node_modules"),
            );
            if edit.changed() {
                self.cfg.skip =
                    self.skip_text.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Names, anywhere in the tree, separated by commas; * works (*.log).")
                        .small()
                        .color(FAINT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::link(ui, "Reset").clicked() {
                        self.cfg.skip = tree::default_skip();
                        self.skip_text = self.cfg.skip.join(", ");
                        changed = true;
                    }
                });
            });
            if changed {
                self.changed_at = Some(now);
            }
        });
    }

    fn style_card(&mut self, ui: &mut Ui, now: f64) {
        card(ui, "How it's written", |ui| {
            let mut changed = false;
            let options: Vec<(Style, &str)> = Style::ALL.iter().map(|&s| (s, s.label())).collect();
            changed |= widgets::segmented(ui, &mut self.cfg.style, &options);
            let hint = match self.cfg.style {
                Style::Tree => {
                    "\u{251C}\u{2500}\u{2500} lines, the way the tree command draws it. Reads right anywhere."
                }
                Style::Markdown => "The tree in a ```text block, so READMEs, GitHub and Discord keep it lined up.",
                Style::Paths => "One path per line (src/main.rs): for scripts, and for AI prompts that want paths.",
            };
            ui.label(RichText::new(hint).small().color(FAINT));
            row(ui, "At most", "Lines in all, then \u{201C}\u{2026} N more lines\u{201D}", |ui| {
                let shown =
                    if self.cfg.max_lines == 0 { "No limit".to_owned() } else { self.cfg.max_lines.to_string() };
                ui.label(RichText::new(shown).font(theme::semibold(14.0)).color(ACID));
                changed |= ui
                    .add(egui::Slider::new(&mut self.cfg.max_lines, 0..=5000).step_by(100.0).show_value(false))
                    .changed();
            });
            if changed {
                self.changed_at = Some(now);
            }
        });
    }

    fn behaviour_card(&mut self, ui: &mut Ui, now: f64) {
        card(ui, "Behavior", |ui| {
            let mut auto = self.autostart;
            row(ui, "Start with Windows", "Quietly, in the tray, for the hotkey", |ui| {
                if toggle(ui, &mut auto).changed() {
                    autostart::set(auto);
                    self.autostart = autostart::is_enabled();
                }
            });
            let mut flags = (self.cfg.card, self.cfg.hello_card);
            row(ui, "Card after each copy", "In the corner: what was copied, and other styles", |ui| {
                if toggle(ui, &mut flags.0).changed() {
                    self.cfg.card = flags.0;
                    self.changed_at = Some(now);
                }
            });
            row(ui, "Hello card at start", "The little donationware note", |ui| {
                if toggle(ui, &mut flags.1).changed() {
                    self.cfg.hello_card = flags.1;
                    self.changed_at = Some(now);
                }
            });
            row(
                ui,
                "Take Twig out of Windows",
                "The right-click entry, start-up and settings. Then delete Twig.exe",
                |ui| {
                    let armed = self.uninstall_armed.is_some();
                    let label = if armed { "Click again to remove" } else { "Remove\u{2026}" };
                    if widgets::ghost(ui, label).clicked() {
                        if armed {
                            self.save_now();
                            win::spawn_self(&["--uninstall"]);
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        } else {
                            self.uninstall_armed = Some(now + 4.0);
                        }
                    }
                },
            );
        });
    }

    fn footer(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let links_w = 430.0;
                ui.add_space(((ui.available_width() - links_w) / 2.0).max(0.0));
                if widgets::link(ui, "acidalchamy.com/donationware").clicked() {
                    win::open_url(crate::HOME_URL);
                }
                if widgets::link(ui, "Source code").clicked() {
                    win::open_url(crate::REPO_URL);
                }
                if widgets::link(ui, "Settings file").clicked() {
                    win::open_url(&config::dir().display().to_string());
                }
                if self.resident_up && widgets::link(ui, "Quit Twig").clicked() {
                    self.save_now();
                    ipc::send(WM_TWIG_QUIT, 0, 0);
                    self.last_poll = 0.0;
                }
            });
            ui.label(
                RichText::new(format!(
                    "Twig {}  \u{00B7}  MIT License  \u{00B7}  Donationware by Acid Alchamy  \u{00B7}  changes save as you go",
                    crate::VERSION
                ))
                .small()
                .color(FAINT),
            );
        });
    }
}
