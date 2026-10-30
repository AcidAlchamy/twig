//! A small drawing kit for the resident's own windows (the area picker and the
//! nab card): a memory DC over a 32-bit DIB, anti-aliased rounded rectangles
//! written straight into pixels, and GDI text. No UI toolkit in the resident.

use crate::win::wide;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection,
    CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_NOPREFIX, DT_PATH_ELLIPSIS,
    DT_SINGLELINE, DeleteDC, DeleteObject, DrawTextW, FF_DONTCARE, GdiFlush, GetTextExtentPoint32W, HBITMAP, HDC,
    HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};

/// A 32-bit top-down bitmap selected into its own memory DC. Pixels are
/// 0xAARRGGBB words (B, G, R, A in memory).
pub struct Canvas {
    pub dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    pub w: i32,
    pub h: i32,
}

impl Canvas {
    pub fn new(w: i32, h: i32) -> Option<Self> {
        let (w, h) = (w.max(1), h.max(1));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            let mut bits: *mut core::ffi::c_void = null_mut();
            let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bmp.is_null() || bits.is_null() {
                DeleteDC(dc);
                return None;
            }
            let old = SelectObject(dc, bmp);
            Some(Self { dc, bmp, old, bits: bits.cast(), w, h })
        }
    }

    /// The pixels, after any pending GDI drawing has landed.
    pub fn px(&mut self) -> &mut [u32] {
        unsafe {
            GdiFlush();
            std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize)
        }
    }

    #[cfg(test)]
    pub fn px_ref(&self) -> &[u32] {
        unsafe {
            GdiFlush();
            std::slice::from_raw_parts(self.bits, (self.w * self.h) as usize)
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bmp);
            DeleteDC(self.dc);
        }
    }
}

/// 0xRRGGBB as a GDI COLORREF (0x00BBGGRR).
pub fn colorref(rgb: u32) -> u32 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

/// Mixes two 0xRRGGBB colours; `t` = 0 gives `a`.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 0xFF) as f32, ((b >> s) & 0xFF) as f32);
        ((x + (y - x) * t + 0.5) as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

/// How much of the pixel at (px, py) a rounded rectangle covers (0..1),
/// anti-aliased over about one pixel.
pub fn cover(px: i32, py: i32, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let (cx, cy) = (px as f32 + 0.5 - (x + w / 2.0), py as f32 + 0.5 - (y + h / 2.0));
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let (qx, qy) = (cx.abs() - (w / 2.0 - r), cy.abs() - (h / 2.0 - r));
    let outside = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
    (0.5 - outside).clamp(0.0, 1.0)
}

/// Paints an opaque rounded rectangle into straight (unmultiplied) pixels,
/// blending its anti-aliased edge into what's there. With `line` > 0, only an
/// outline that wide.
pub fn rrect(px: &mut [u32], stride: i32, x: f32, y: f32, w: f32, h: f32, r: f32, rgb: u32, line: f32) {
    let rows = px.len() as i32 / stride.max(1);
    let (x0, y0) = ((x.floor() as i32).max(0), (y.floor() as i32).max(0));
    let (x1, y1) = (((x + w).ceil() as i32).min(stride), ((y + h).ceil() as i32).min(rows));
    for yy in y0..y1 {
        for xx in x0..x1 {
            let mut c = cover(xx, yy, x, y, w, h, r);
            if line > 0.0 {
                c -= cover(xx, yy, x + line, y + line, w - 2.0 * line, h - 2.0 * line, (r - line).max(0.0));
            }
            if c > 0.0 {
                let p = &mut px[(yy * stride + xx) as usize];
                *p = 0xFF00_0000 | mix(*p & 0xFF_FFFF, rgb, c);
            }
        }
    }
}

/// Turns straight pixels into premultiplied ones with alpha from `mask`.
pub fn apply_mask(px: &mut [u32], mask: &[u8]) {
    for (p, &a) in px.iter_mut().zip(mask) {
        if a == 255 {
            *p |= 0xFF00_0000;
        } else {
            let k = a as u32;
            let ch = |s: u32| ((((*p >> s) & 0xFF) * k + 127) / 255) << s;
            *p = (k << 24) | ch(16) | ch(8) | ch(0);
        }
    }
}

/// A Segoe UI font at `px` pixels tall (weight 400 regular, 600 semibold, 700 bold).
pub struct Font(HFONT);

impl Font {
    pub fn new(px: i32, weight: i32) -> Self {
        Self::face("Segoe UI", px, weight)
    }

    /// Consolas, for the tree preview.
    pub fn mono(px: i32) -> Self {
        Self::face("Consolas", px, 400)
    }

    fn face(face: &str, px: i32, weight: i32) -> Self {
        let face = wide(face);
        Self(unsafe {
            CreateFontW(
                -px.max(6),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as _,
                OUT_DEFAULT_PRECIS as _,
                CLIP_DEFAULT_PRECIS as _,
                CLEARTYPE_QUALITY as _,
                (DEFAULT_PITCH | FF_DONTCARE) as _,
                face.as_ptr(),
            )
        })
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.0) };
    }
}

/// Draws one line of text inside `r`: single line, with an ellipsis at the
/// end, or in the middle when `align` has DT_PATH_ELLIPSIS (keeps ".png").
pub fn text(dc: HDC, s: &str, r: RECT, font: &Font, rgb: u32, align: u32) {
    let mut buf: Vec<u16> = s.encode_utf16().collect();
    let mut r = r;
    unsafe {
        let old = SelectObject(dc, font.0);
        SetBkMode(dc, TRANSPARENT as _);
        SetTextColor(dc, colorref(rgb));
        let ellipsis = if align & DT_PATH_ELLIPSIS != 0 { 0 } else { DT_END_ELLIPSIS };
        DrawTextW(dc, buf.as_mut_ptr(), buf.len() as i32, &mut r, DT_SINGLELINE | DT_NOPREFIX | ellipsis | align);
        SelectObject(dc, old);
    }
}

pub fn text_width(dc: HDC, s: &str, font: &Font) -> i32 {
    let buf: Vec<u16> = s.encode_utf16().collect();
    let mut size = windows_sys::Win32::Foundation::SIZE::default();
    unsafe {
        let old = SelectObject(dc, font.0);
        GetTextExtentPoint32W(dc, buf.as_ptr(), buf.len() as i32, &mut size);
        SelectObject(dc, old);
    }
    size.cx
}

/// `s` shortened in the middle to fit `max_w` pixels, keeping its last `tail`
/// characters, so a file name keeps its time and extension.
pub fn fit_middle(dc: HDC, s: &str, font: &Font, max_w: i32, tail: usize) -> String {
    if text_width(dc, s, font) <= max_w {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    let tail = tail.min(chars.len());
    let end: String = chars[chars.len() - tail..].iter().collect();
    let mut head = chars.len() - tail;
    while head > 0 {
        head -= 1;
        let candidate = format!("{}\u{2026}{end}", chars[..head].iter().collect::<String>().trim_end());
        if text_width(dc, &candidate, font) <= max_w {
            return candidate;
        }
    }
    format!("\u{2026}{end}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_is_solid_inside_and_soft_at_corners() {
        assert_eq!(cover(10, 10, 0.0, 0.0, 40.0, 20.0, 6.0), 1.0);
        assert_eq!(cover(60, 10, 0.0, 0.0, 40.0, 20.0, 6.0), 0.0);
        let corner = cover(0, 0, 0.0, 0.0, 40.0, 20.0, 6.0);
        assert!(corner < 0.1, "{corner}");
        let edge = cover(0, 10, 0.0, 0.0, 40.0, 20.0, 6.0);
        assert!(edge > 0.9, "{edge}");
    }

    #[test]
    fn colours() {
        assert_eq!(colorref(0x112233), 0x332211);
        assert_eq!(mix(0x000000, 0xFFFFFF, 0.5), 0x808080);
        let mut px = [0x00FF_0000u32, 0x00FF_FFFF];
        apply_mask(&mut px, &[255, 128]);
        assert_eq!(px[0], 0xFFFF_0000);
        assert_eq!(px[1] >> 24, 128);
        assert_eq!((px[1] >> 16) & 0xFF, 128);
    }
}
