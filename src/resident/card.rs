//! The card: a small note in the corner after each copy. The top of the tree
//! in miniature, what's in it, what was left out, and buttons to copy it again
//! in another style or view it whole. It fades away after a few seconds unless
//! you're pointing at it, and never takes the keyboard from the app you're in.
//!
//! Drawn by hand into a per-pixel-alpha window: no UI toolkit in the resident.

use crate::paint::{self, Canvas, Font, apply_mask, cover, rrect};
use crate::tree::Style;
use crate::win::{self, wide};
use std::ptr::{null, null_mut};
use std::time::Instant;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, DT_CENTER, DT_LEFT, DT_VCENTER};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HWND_TOPMOST, IDC_ARROW, IDC_HAND, LoadCursorW, RegisterClassExW, SW_HIDE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetCursor, SetWindowPos, ShowWindow, ULW_ALPHA,
    UpdateLayeredWindow, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETCURSOR, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const W: f32 = 420.0;
const H: f32 = 112.0;
const RADIUS: f32 = 12.0;
const SHOW_MS: f32 = 5_000.0;
const FADE_IN_MS: f32 = 160.0;
const FADE_OUT_MS: f32 = 220.0;
/// The tree preview's well and the text column.
const WELL: (f32, f32, f32, f32) = (12.0, 12.0, 118.0, H - 24.0);
const X0: f32 = 144.0;

/// Not in windows-sys's WindowsAndMessaging; it lives with the common controls.
const WM_MOUSELEAVE: u32 = 0x02A3;

const BG: u32 = 0x161320;
const WELL_BG: u32 = 0x0A0810;
const HI: u32 = 0x1F1A2C;
const LINE: u32 = 0x2A243B;
const TEXT: u32 = 0xEEECF4;
const MUTED: u32 = 0xA39DB6;
const FAINT: u32 = 0x6E6884;
const ACID: u32 = 0xC6FF3A;
const VIOLET: u32 = 0x8B3DFF;
const PINK: u32 = 0xFF2E7E;
const BAD: u32 = 0xFF5A5A;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Reading,
    Copied,
    Failed(String),
    /// Not a copy: a short note (title, line). "Open a folder in File Explorer first".
    Note(String, String),
}

/// What a click on the card asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Nothing,
    Again(Style),
    View,
    Close,
}

#[derive(Clone, Debug, Default)]
pub struct Info {
    pub id: u32,
    pub folder: String,
    pub style: Option<Style>,
    /// The first lines of the tree, for the preview.
    pub preview: Vec<String>,
    pub lines: usize,
    pub folders: usize,
    pub files: usize,
    /// Names from the skip list that were really there.
    pub skipped: Vec<String>,
    pub gitignore: bool,
}

struct Fonts {
    scale: f32,
    title: Font,
    name: Font,
    small: Font,
    mono: Font,
}

pub struct Card {
    hwnd: HWND,
    canvas: Option<Canvas>,
    mask: Vec<u8>,
    fonts: Option<Fonts>,
    info: Info,
    status: Status,
    scale: f32,
    pos: POINT,
    began: Instant,
    last_tick: Instant,
    last_draw: Instant,
    shown_ms: f32,
    leaving: Option<Instant>,
    visible: bool,
    hover: Hit,
    pointer_in: bool,
    hits: Vec<(Hit, RECT)>,
}

impl Card {
    pub fn new() -> Self {
        let hinst = unsafe { GetModuleHandleW(null()) };
        let class = wide("Twig.Card");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(card_proc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        let hwnd = unsafe {
            RegisterClassExW(&wc);
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                wide("Twig card").as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                hinst,
                null(),
            )
        };
        let now = Instant::now();
        Self {
            hwnd,
            canvas: None,
            mask: Vec::new(),
            fonts: None,
            info: Info::default(),
            status: Status::Reading,
            scale: 1.0,
            pos: POINT { x: 0, y: 0 },
            began: now,
            last_tick: now,
            last_draw: now,
            shown_ms: 0.0,
            leaving: None,
            visible: false,
            hover: Hit::Nothing,
            pointer_in: false,
            hits: Vec::new(),
        }
    }

    pub fn active(&self) -> bool {
        self.visible
    }

    pub fn id(&self) -> u32 {
        self.info.id
    }

    /// Shows the card (or brings it back to a full countdown) on the screen with the pointer.
    pub fn show(&mut self, info: Info, status: Status) {
        let Some(mon) = win::monitor_at(win::cursor()) else { return };
        self.prepare(info, status, mon.scale());
        let (w, h) = ((W * self.scale).round() as i32, (H * self.scale).round() as i32);
        let margin = (16.0 * self.scale) as i32;
        self.pos = POINT { x: mon.work.right - w - margin, y: mon.work.bottom - h - margin };
        self.render();
        if !self.visible {
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            };
            self.visible = true;
        }
    }

    fn prepare(&mut self, info: Info, status: Status, scale: f32) {
        // Coming back while already on screen: no second fade-in.
        let fresh = !self.visible;
        self.scale = scale;
        let s = self.scale;
        let (w, h) = ((W * s).round() as i32, (H * s).round() as i32);
        if self.canvas.as_ref().is_none_or(|c| c.w != w || c.h != h) {
            self.canvas = Canvas::new(w, h);
            let r = RADIUS * s;
            self.mask = (0..w * h)
                .map(|i| (cover(i % w, i / w, 0.0, 0.0, w as f32, h as f32, r) * 255.0 + 0.5) as u8)
                .collect();
        }
        if self.fonts.as_ref().is_none_or(|f| f.scale != s) {
            self.fonts = Some(Fonts {
                scale: s,
                title: Font::new((15.5 * s) as i32, 700),
                name: Font::new((13.5 * s) as i32, 600),
                small: Font::new((12.0 * s) as i32, 400),
                mono: Font::mono((10.0 * s) as i32),
            });
        }
        self.info = info;
        self.status = status;
        let now = Instant::now();
        if fresh {
            self.began = now;
        }
        self.last_tick = now;
        self.shown_ms = 0.0;
        self.leaving = None;
    }

    /// The copy for `id` finished (or failed). Ignored if the card moved on to another copy.
    pub fn update(&mut self, info: Info, status: Status) {
        if self.info.id == info.id && self.visible {
            self.info = info;
            self.status = status;
            self.shown_ms = 0.0;
            self.render();
        }
    }

    /// Hides straight away. Returns whether it was on screen.
    pub fn hide(&mut self) -> bool {
        let was = self.visible;
        if was {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
        self.visible = false;
        self.leaving = None;
        self.pointer_in = false;
        self.hover = Hit::Nothing;
        was
    }

    /// Fades out.
    pub fn dismiss(&mut self) {
        if self.visible && self.leaving.is_none() {
            self.leaving = Some(Instant::now());
        }
    }

    /// Fading in or out (wants frames); otherwise it only needs a redraw now and then.
    pub fn animating(&self) -> bool {
        self.visible && (self.began.elapsed().as_secs_f32() * 1000.0 < FADE_IN_MS || self.leaving.is_some())
    }

    pub fn tick(&mut self) {
        if !self.visible {
            return;
        }
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32() * 1000.0;
        self.last_tick = now;
        // Ask where the pointer is rather than trusting enter/leave messages.
        if let Some(c) = &self.canvas {
            let p = win::cursor();
            let r = RECT { left: self.pos.x, top: self.pos.y, right: self.pos.x + c.w, bottom: self.pos.y + c.h };
            self.pointer_in = win::contains(&r, p);
        }
        // A tree being read never times out.
        if !self.pointer_in && self.status != Status::Reading {
            self.shown_ms += dt;
        }
        if self.leaving.is_none() && self.shown_ms >= SHOW_MS {
            self.leaving = Some(now);
        }
        if self.leaving.is_some_and(|t| t.elapsed().as_secs_f32() * 1000.0 >= FADE_OUT_MS) {
            self.hide();
            self.canvas = None; // give the memory back until the next copy
            self.mask.clear();
            return;
        }
        if now.duration_since(self.last_draw).as_millis() >= 125 {
            self.render();
        } else if self.animating() {
            self.present();
        }
    }

    fn motion(&self) -> (f32, i32) {
        let ease = |t: f32| 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3);
        let t_in = ease(self.began.elapsed().as_secs_f32() * 1000.0 / FADE_IN_MS);
        let t_out = self.leaving.map_or(0.0, |t| ease(t.elapsed().as_secs_f32() * 1000.0 / FADE_OUT_MS));
        let alpha = t_in * (1.0 - t_out);
        let slide = ((1.0 - t_in) * 12.0 + t_out * 6.0) * self.scale;
        (alpha, slide.round() as i32)
    }

    fn present(&self) {
        let Some(canvas) = &self.canvas else { return };
        let (alpha, slide) = self.motion();
        let pos = POINT { x: self.pos.x, y: self.pos.y + slide };
        let size = SIZE { cx: canvas.w, cy: canvas.h };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: (alpha * 255.0) as u8,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe { UpdateLayeredWindow(self.hwnd, null_mut(), &pos, &size, canvas.dc, &src, 0, &blend, ULW_ALPHA) };
    }

    /// The buttons for this state: the two other styles, then View.
    fn buttons(&self) -> Vec<(Hit, &'static str, (f32, f32, f32, f32))> {
        let Some(style) = self.info.style.filter(|_| self.status == Status::Copied) else { return Vec::new() };
        let mut out = Vec::new();
        let mut x = X0;
        for other in Style::ALL.into_iter().filter(|&s| s != style) {
            let (label, w) = match other {
                Style::Tree => ("As a tree", 82.0),
                Style::Markdown => ("Markdown", 86.0),
                Style::Paths => ("Paths", 62.0),
            };
            out.push((Hit::Again(other), label, (x, H - 34.0, w, 24.0)));
            x += w + 6.0;
        }
        out.push((Hit::View, "View", (x, H - 34.0, 58.0, 24.0)));
        out
    }

    fn render(&mut self) {
        self.last_draw = Instant::now();
        let buttons = self.buttons();
        let Some(fonts) = &self.fonts else { return };
        let Some(canvas) = self.canvas.as_mut() else { return };
        let info = &self.info;
        let s = self.scale;
        let (w, h) = (canvas.w, canvas.h);

        let close = (W - 32.0, 8.0, 24.0, 24.0);
        let hover = self.hover;
        let sc = |v: f32| v * s;
        let at = |x: f32, y: f32, bw: f32, bh: f32| RECT {
            left: (x * s) as i32,
            top: (y * s) as i32,
            right: ((x + bw) * s) as i32,
            bottom: ((y + bh) * s) as i32,
        };
        let mut hits = vec![(Hit::Close, at(close.0, close.1, close.2, close.3))];
        hits.extend(buttons.iter().map(|&(k, _, (x, y, bw, bh))| (k, at(x, y, bw, bh))));
        self.hits = hits;

        {
            let px = canvas.px();
            px.fill(0xFF00_0000 | BG);
            // The brand line along the top.
            let rule = (3.0 * s).round() as i32;
            for y in 0..rule.min(h) {
                for x in 0..w {
                    let t = x as f32 / (w - 1).max(1) as f32;
                    let c = if t < 0.6 {
                        paint::mix(VIOLET, PINK, t / 0.6)
                    } else {
                        paint::mix(PINK, ACID, (t - 0.6) / 0.4)
                    };
                    px[(y * w + x) as usize] = 0xFF00_0000 | c;
                }
            }
            rrect(px, w, 0.0, 0.0, w as f32, h as f32, RADIUS * s, LINE, 1.0);
            let (wx, wy, ww, wh) = (sc(WELL.0), sc(WELL.1), sc(WELL.2), sc(WELL.3));
            rrect(px, w, wx, wy, ww, wh, sc(8.0), WELL_BG, 0.0);
            rrect(px, w, wx, wy, ww, wh, sc(8.0), LINE, 1.0);
            for &(kind, _, (x, y, bw, bh)) in &buttons {
                rrect(px, w, sc(x), sc(y), sc(bw), sc(bh), sc(8.0), HI, 0.0);
                rrect(px, w, sc(x), sc(y), sc(bw), sc(bh), sc(8.0), if hover == kind { ACID } else { LINE }, 1.0);
            }
            if hover == Hit::Close {
                rrect(px, w, sc(close.0), sc(close.1), sc(close.2), sc(close.3), sc(7.0), HI, 0.0);
            }
            // Time left, a thin lime line along the bottom.
            if self.status != Status::Reading {
                let left = (1.0 - self.shown_ms / SHOW_MS).clamp(0.0, 1.0);
                let (bx, by, bw) = (sc(RADIUS), h as f32 - sc(3.0), (w as f32 - 2.0 * sc(RADIUS)) * left);
                if bw > 0.5 {
                    let c = paint::mix(BG, ACID, if self.pointer_in { 0.35 } else { 0.8 });
                    rrect(px, w, bx, by, bw, sc(2.0), sc(1.0), c, 0.0);
                }
            }
        }

        let dc = canvas.dc;
        // The tree in miniature (or a little sprig while there's none yet).
        let sprig = [
            "\u{251C}\u{2500}\u{2500} \u{2026}",
            "\u{2502}   \u{2514}\u{2500}\u{2500} \u{2026}",
            "\u{2514}\u{2500}\u{2500} \u{2026}",
        ];
        let (lines, color): (Vec<&str>, u32) = if info.preview.is_empty() {
            (sprig.to_vec(), FAINT)
        } else {
            (info.preview.iter().map(String::as_str).collect(), MUTED)
        };
        let line_h = 12.5;
        let fits = ((WELL.3 - 14.0) / line_h) as usize;
        for (i, line) in lines.iter().take(fits).enumerate() {
            let r = at(WELL.0 + 8.0, WELL.1 + 7.0 + i as f32 * line_h, WELL.2 - 12.0, line_h);
            paint::text(dc, line, r, &fonts.mono, if i == 0 { TEXT } else { color }, DT_LEFT | DT_VCENTER);
        }

        let (title, title_color) = match &self.status {
            Status::Reading => ("Reading\u{2026}".to_owned(), TEXT),
            Status::Copied => ("Copied".to_owned(), TEXT),
            Status::Failed(_) => ("Couldn't copy".to_owned(), BAD),
            Status::Note(t, _) => (t.clone(), TEXT),
        };
        let title_r = at(X0, 10.0, W - X0 - 40.0, 24.0);
        paint::text(dc, &title, title_r, &fonts.title, title_color, DT_LEFT | DT_VCENTER);
        if !info.folder.is_empty() {
            let tw = paint::text_width(dc, &title, &fonts.title);
            let r = RECT { left: title_r.left + tw + (7.0 * s) as i32, ..title_r };
            let shown = paint::fit_middle(dc, &format!("\u{00B7}  {}", info.folder), &fonts.name, r.right - r.left, 8);
            paint::text(dc, &shown, r, &fonts.name, MUTED, DT_LEFT | DT_VCENTER);
        }
        paint::text(
            dc,
            "\u{00D7}",
            at(close.0, close.1, close.2, close.3),
            &fonts.title,
            if hover == Hit::Close { TEXT } else { FAINT },
            DT_CENTER | DT_VCENTER,
        );

        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let (line2, line3, color3) = match &self.status {
            Status::Reading => ("Walking the folder\u{2026}".to_owned(), String::new(), FAINT),
            Status::Copied => {
                let style = info.style.map_or("", |s| s.label());
                let line2 = format!(
                    "{style}  \u{00B7}  {}  \u{00B7}  {}  \u{00B7}  {}",
                    plural(info.lines, "line", "lines"),
                    plural(info.folders, "folder", "folders"),
                    plural(info.files, "file", "files")
                );
                let skipped = info.skipped.join(", ");
                let line3 = match (skipped.is_empty(), info.gitignore) {
                    (false, true) => format!("Skips {skipped} + .gitignore"),
                    (false, false) => format!("Skips {skipped}"),
                    (true, true) => "Follows .gitignore. Paste it anywhere.".to_owned(),
                    (true, false) => "Paste it anywhere.".to_owned(),
                };
                (line2, line3, FAINT)
            }
            Status::Failed(why) => (why.clone(), String::new(), BAD),
            Status::Note(_, line) => (line.clone(), String::new(), FAINT),
        };
        paint::text(dc, &line2, at(X0, 36.0, W - X0 - 14.0, 20.0), &fonts.small, TEXT, DT_LEFT | DT_VCENTER);
        if !line3.is_empty() {
            paint::text(dc, &line3, at(X0, 54.0, W - X0 - 14.0, 18.0), &fonts.small, color3, DT_LEFT | DT_VCENTER);
        }
        for &(kind, label, (x, y, bw, bh)) in &buttons {
            paint::text(
                dc,
                label,
                at(x, y, bw, bh),
                &fonts.small,
                if hover == kind { ACID } else { TEXT },
                DT_CENTER | DT_VCENTER,
            );
        }

        apply_mask(canvas.px(), &self.mask);
        self.present();
    }

    fn hit_at(&self, x: i32, y: i32) -> Hit {
        self.hits.iter().find(|(_, r)| win::contains(r, POINT { x, y })).map_or(Hit::Nothing, |(k, _)| *k)
    }

    /// Mouse over the card; returns a click's target on button-up.
    pub fn mouse(&mut self, msg: u32, x: i32, y: i32) -> Option<Hit> {
        match msg {
            WM_MOUSEMOVE => {
                if !self.pointer_in {
                    self.pointer_in = true;
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };
                    unsafe { TrackMouseEvent(&mut tme) };
                }
                let hit = self.hit_at(x, y);
                if hit != self.hover {
                    self.hover = hit;
                    self.render();
                }
                None
            }
            WM_MOUSELEAVE => {
                self.pointer_in = false;
                self.hover = Hit::Nothing;
                self.render();
                None
            }
            WM_LBUTTONUP => Some(self.hit_at(x, y)),
            _ => None,
        }
    }

    pub fn wants_hand(&self) -> bool {
        self.hover != Hit::Nothing
    }
}

unsafe extern "system" fn card_proc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_MOUSEACTIVATE => return 3, // MA_NOACTIVATE: clicks never take focus from your app
        WM_SETCURSOR => {
            let hand = super::card_wants_hand();
            unsafe { SetCursor(LoadCursorW(null_mut(), if hand { IDC_HAND } else { IDC_ARROW })) };
            return 1;
        }
        WM_MOUSEMOVE | WM_MOUSELEAVE | WM_LBUTTONUP => {
            let (x, y) = ((lparam & 0xFFFF) as i16 as i32, ((lparam >> 16) & 0xFFFF) as i16 as i32);
            super::card_mouse(msg, x, y);
            return 0;
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The card for the README, drawn by the real code (never shown on screen):
    /// `TWIG_DEMO=art/build cargo test demo_card -- --ignored` writes card.png there.
    #[test]
    #[ignore = "writes README art; needs TWIG_DEMO"]
    fn demo_card() {
        let dir = std::path::PathBuf::from(std::env::var("TWIG_DEMO").expect("TWIG_DEMO=<folder>"));
        let preview = [
            "my-app/",
            "\u{251C}\u{2500}\u{2500} .github/",
            "\u{251C}\u{2500}\u{2500} src/",
            "\u{2502}   \u{251C}\u{2500}\u{2500} ui/",
            "\u{2502}   \u{2514}\u{2500}\u{2500} main.rs",
            "\u{251C}\u{2500}\u{2500} tests/",
            "\u{2514}\u{2500}\u{2500} README.md",
        ];
        let mut card = Card::new();
        let info = Info {
            id: 1,
            folder: "my-app".into(),
            style: Some(Style::Tree),
            preview: preview.map(String::from).to_vec(),
            lines: 18,
            folders: 6,
            files: 12,
            skipped: vec!["node_modules".into(), ".git".into()],
            gitignore: true,
        };
        card.prepare(info, Status::Copied, 2.0);
        card.shown_ms = SHOW_MS * 0.35;
        card.hover = Hit::Again(Style::Markdown);
        card.render();
        let c = card.canvas.as_ref().unwrap();
        // Premultiplied back to straight alpha, then RGBA for the PNG.
        let mut rgba = Vec::with_capacity((c.w * c.h * 4) as usize);
        for &p in c.px_ref() {
            let a = p >> 24;
            let ch = |sh: u32| if a == 0 { 0 } else { ((((p >> sh) & 0xFF) * 255 + a / 2) / a).min(255) as u8 };
            rgba.extend_from_slice(&[ch(16), ch(8), ch(0), a as u8]);
        }
        let file = std::fs::File::create(dir.join("card.png")).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), c.w as u32, c.h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    }
}
