"""Builds the README pictures in docs/img.

    python art/icons.py
    TWIG_DEMO=art/build cargo test demo -- --ignored     (the card, drawn by Twig's own code)
    python art/readme.py

The Explorer window and its menu are drawn here from scratch over a made-up
project; no real screen is ever captured for these. The box comes from the
website's renderer (tools/donationware/render_covers.py in the site's repo).
"""
import pathlib
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent
BUILD = ROOT / "build"
IMG = ROOT.parent / "docs" / "img"
SITE_BOX = pathlib.Path.home() / "dev" / "acidalchamy.com" / "app" / "donationware" / "assets" / "twig" / "box.png"
F = "C:/Windows/Fonts/"
BG = (13, 11, 20)
ACID, PINK, VIOLET, GREEN = (198, 255, 58), (255, 46, 126), (139, 61, 255), (75, 227, 138)
TEXT, MUTED, FAINT = (238, 236, 244), (163, 157, 182), (110, 104, 132)


def font(name, size):
    return ImageFont.truetype(F + name, size)


def glow(img, at, radius, color, strength):
    layer = Image.new("RGBA", img.size, (0, 0, 0, 0))
    ImageDraw.Draw(layer).ellipse((at[0] - radius, at[1] - radius, at[0] + radius, at[1] + radius), fill=color + (strength,))
    img.alpha_composite(layer.filter(ImageFilter.GaussianBlur(radius * 0.6)))


def grid(img, step):
    g = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(g)
    for x in range(0, img.width, step):
        d.line([(x, 0), (x, img.height)], fill=(255, 255, 255, 9))
    for y in range(0, img.height, step):
        d.line([(0, y), (img.width, y)], fill=(255, 255, 255, 9))
    img.alpha_composite(g)  # drawing on RGBA replaces pixels; compositing blends


def brand_rule(img, y):
    w = img.width
    rule = Image.new("RGBA", (w, 3))
    for x in range(w):
        t = x / (w - 1)
        a, b, k = (VIOLET, PINK, t / 0.6) if t < 0.6 else (PINK, ACID, (t - 0.6) / 0.4)
        ImageDraw.Draw(rule).line([(x, 0), (x, 2)], fill=tuple(int(a[i] + (b[i] - a[i]) * k) for i in range(3)) + (255,))
    img.alpha_composite(rule, (0, y))


def shadowed(img, layer, at, radius=14):
    shadow = Image.new("RGBA", img.size, (0, 0, 0, 0))
    x, y = at
    ImageDraw.Draw(shadow).rounded_rectangle((x + 6, y + 14, x + layer.width + 6, y + layer.height + 14), radius, fill=(0, 0, 0, 170))
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(14)))
    img.alpha_composite(layer, at)


def card(scale):
    c = Image.open(BUILD / "card.png").convert("RGBA")
    return c.resize((int(c.width * scale), int(c.height * scale)), Image.LANCZOS)


def banner():
    w, h = 1280, 400
    img = Image.new("RGBA", (w, h), BG + (255,))
    glow(img, (180, 200), 220, GREEN, 90)
    glow(img, (1120, 330), 200, VIOLET, 110)
    grid(img, 32)
    d = ImageDraw.Draw(img)
    icon = Image.open(ROOT / "twig-icon-512.png").convert("RGBA").resize((200, 200), Image.LANCZOS)
    img.alpha_composite(icon, (64, 92))
    d.text((300, 84), "Twig", font=font("segoeuib.ttf", 112), fill=TEXT)
    d.text((306, 228), "Right-click a folder, get its tree.", font=font("seguisb.ttf", 30), fill=ACID)
    d.text((306, 274), "For a chat, a README or an AI prompt.", font=font("segoeui.ttf", 21), fill=MUTED)
    d.text((306, 306), "Free donationware for Windows 10 & 11  \u00b7  built in Rust", font=font("segoeui.ttf", 21), fill=MUTED)
    c = card(0.45)
    shadowed(img, c, (w - c.width - 40, h - c.height - 34))
    brand_rule(img, h - 3)
    img.convert("RGB").save(IMG / "banner.png", optimize=True)


def folder_icon(d, x, y, s=1.0):
    d.rounded_rectangle((x, y + 3 * s, x + 18 * s, y + 15 * s), 2, fill=(232, 189, 74))
    d.rounded_rectangle((x, y, x + 8 * s, y + 5 * s), 2, fill=(232, 189, 74))
    d.rounded_rectangle((x, y + 5 * s, x + 18 * s, y + 15 * s), 2, fill=(248, 215, 117))


def file_icon(d, x, y):
    d.polygon([(x + 2, y), (x + 11, y), (x + 15, y + 4), (x + 15, y + 17), (x + 2, y + 17)], fill=(222, 222, 232))
    d.polygon([(x + 11, y), (x + 15, y + 4), (x + 11, y + 4)], fill=(170, 170, 186))


def menu():
    """Explorer's classic right-click menu, on the empty space inside a made-up project
    (the folder that's open), and the card after."""
    w, h = 1200, 640
    img = Image.new("RGBA", (w, h), (24, 18, 40, 255))
    glow(img, (220, 120), 360, VIOLET, 120)
    glow(img, (1040, 560), 320, PINK, 80)
    glow(img, (900, 140), 260, GREEN, 50)
    d = ImageDraw.Draw(img)

    # The Explorer window.
    win = Image.new("RGBA", (760, 470), (0, 0, 0, 0))
    wd = ImageDraw.Draw(win)
    wd.rounded_rectangle((0, 0, 759, 469), 10, fill=(32, 32, 32), outline=(60, 60, 64))
    wd.rounded_rectangle((0, 0, 759, 44), 10, fill=(26, 26, 26))
    wd.rectangle((0, 30, 759, 44), fill=(26, 26, 26))
    wd.rounded_rectangle((10, 8, 200, 44), 8, fill=(44, 44, 46))
    folder_icon(wd, 22, 18, 0.9)
    wd.text((46, 15), "my-app", font=font("segoeui.ttf", 15), fill=(240, 240, 240))
    wd.text((178, 13), "\u00d7", font=font("segoeui.ttf", 16), fill=(170, 170, 170))
    wd.text((214, 12), "+", font=font("segoeui.ttf", 18), fill=(170, 170, 170))
    for i, glyph in enumerate(["\u2014", "\u25a1", "\u00d7"]):
        wd.text((640 + i * 40, 10), glyph, font=font("segoeui.ttf", 16), fill=(200, 200, 200))
    wd.rounded_rectangle((120, 56, 560, 86), 6, fill=(44, 44, 46))
    wd.text((134, 61), "This PC  \u203a  Code  \u203a  my-app", font=font("segoeui.ttf", 15), fill=(230, 230, 230))
    wd.rounded_rectangle((572, 56, 744, 86), 6, fill=(44, 44, 46))
    wd.text((586, 61), "Search my-app", font=font("segoeui.ttf", 14), fill=(150, 150, 150))
    wd.line([(0, 98), (759, 98)], fill=(52, 52, 56))
    # Navigation pane.
    for i, name in enumerate(["Home", "Desktop", "Documents", "Code", "This PC"]):
        y = 116 + i * 30
        folder_icon(wd, 18, y + 2, 0.8)
        wd.text((44, y), name, font=font("segoeui.ttf", 14), fill=(220, 220, 220))
    wd.line([(170, 98), (170, 469)], fill=(52, 52, 56))
    wd.text((190, 108), "Name", font=font("segoeui.ttf", 13), fill=(160, 160, 160))
    wd.text((520, 108), "Date modified", font=font("segoeui.ttf", 13), fill=(160, 160, 160))
    rows = [(".github", True), ("node_modules", True), ("src", True), ("tests", True),
            (".gitignore", False), ("Cargo.toml", False), ("LICENSE", False), ("README.md", False)]
    for i, (name, is_dir) in enumerate(rows):
        y = 136 + i * 32
        (folder_icon if is_dir else file_icon)(wd, 192, y + 1)
        wd.text((222, y), name, font=font("segoeui.ttf", 15), fill=(235, 235, 235))
        wd.text((520, y), "10/29/2026 9:14 PM", font=font("segoeui.ttf", 14), fill=(150, 150, 150))
    shadowed(img, win, (40, 40), 12)

    # The classic menu (Windows 11: Show more options, or Shift+right-click).
    items = ["View", "Sort by", "Group by", "Refresh", None, "Copy tree with Twig", "Open in Terminal", None,
             "Paste", "Give access to", "New", None, "Properties"]
    mw, row_h = 300, 30
    mh = sum(row_h if t else 11 for t in items) + 12
    m = Image.new("RGBA", (mw, mh), (0, 0, 0, 0))
    md = ImageDraw.Draw(m)
    md.rounded_rectangle((0, 0, mw - 1, mh - 1), 8, fill=(43, 43, 43), outline=(70, 70, 72))
    icon16 = Image.open(BUILD / "twig-24.png").convert("RGBA").resize((18, 18), Image.LANCZOS)
    y, ours = 6, 0
    for t in items:
        if t is None:
            md.line([(10, y + 5), (mw - 10, y + 5)], fill=(70, 70, 72))
            y += 11
            continue
        if t == "Copy tree with Twig":
            md.rounded_rectangle((5, y, mw - 6, y + row_h - 2), 5, fill=(62, 62, 66))
            md.rounded_rectangle((5, y, mw - 6, y + row_h - 2), 5, outline=ACID, width=2)
            m.alpha_composite(icon16, (16, y + 5))
            ours = y
        md.text((46, y + 5), t, font=font("segoeui.ttf", 15), fill=(240, 240, 240))
        if t in ("View", "Sort by", "Group by", "Give access to", "New"):
            md.text((mw - 26, y + 4), "\u203a", font=font("segoeui.ttf", 16), fill=(200, 200, 200))
        y += row_h
    mx, my = 440, 220
    shadowed(img, m, (mx, my), 8)
    # The pointer on our item.
    px, py = mx + 200, my + ours + 16
    d.polygon([(px, py), (px, py + 26), (px + 7, py + 19), (px + 12, py + 30), (px + 17, py + 28), (px + 12, py + 17), (px + 21, py + 17)],
              fill=(255, 255, 255), outline=(10, 10, 14))

    # And the card that comes up after, in the corner.
    c = card(0.62)
    shadowed(img, c, (w - c.width - 30, h - c.height - 26))
    img.convert("RGB").save(IMG / "menu.png", optimize=True)


def settings():
    """The window alone: a capture's outer pixels and rounded corners hold whatever was behind it."""
    img = Image.open(BUILD / "settings-top.png").convert("RGBA")
    img = img.crop((2, 2, img.width - 3, img.height - 3))
    mask = Image.new("L", (img.width * 4, img.height * 4), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, mask.width - 1, mask.height - 1), radius=36, fill=255)
    img.putalpha(mask.resize(img.size, Image.LANCZOS))
    img.save(IMG / "settings.png", optimize=True)


def main():
    IMG.mkdir(parents=True, exist_ok=True)
    banner()
    menu()
    Image.open(BUILD / "card.png").save(IMG / "card.png", optimize=True)
    if SITE_BOX.exists():
        Image.open(SITE_BOX).save(IMG / "box.png", optimize=True)
    if (BUILD / "settings-top.png").exists():
        settings()
    print("docs/img written")


if __name__ == "__main__":
    main()
