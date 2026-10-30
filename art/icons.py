"""Renders Twig's icon from SVG at every size (crisp, not downscaled) and packs
res/twig.ico and res/twig-paused.ico. Needs Chrome (or Edge) and Pillow.

    python art/icons.py
"""
import pathlib, subprocess, tempfile
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Headless screenshots: Chrome first (Edge's headless mode stopped writing them on some PCs).
BROWSER = next(
    p for p in [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ] if pathlib.Path(p).exists()
)
SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]


def svg(px: int, paused: bool) -> str:
    """The `tree` command's own shape: a root, a stem, two branches; the last one a lime twig."""
    small = px <= 24
    top, bottom = ("#5B5B6E", "#2E2E3A") if paused else ("#8B3DFF", "#FF2E7E")
    lime = "#B9B9C8" if paused else "#C6FF3A"
    paper = "#D9D9E3" if paused else "#FFFFFF"
    if small:
        # Fewer, fatter strokes so it still reads at 16 px.
        root, stem_x, stem_w = (40, 30, 112, 66, 16), 76, 30
        nodes = [(124, 104, 220, 144, paper), (124, 178, 220, 218, lime)]
        rows = [124, 198]
    else:
        root, stem_x, stem_w = (48, 40, 128, 88, 14), 80, 16
        nodes = [(126, 118, 212, 154, paper), (126, 172, 212, 208, lime)]
        rows = [136, 190]
    x0, y0, x1, y1, rr = root
    last = rows[-1]
    branches = "".join(f'<path d="M{stem_x} {y} H{nodes[0][0] + 6}" stroke="{paper}" stroke-width="{stem_w}" fill="none"/>' for y in rows[:-1])
    elbow = (f'<path d="M{stem_x} {y1 - 4} V{last - stem_w} Q{stem_x} {last} {stem_x + stem_w} {last} H{nodes[-1][0] + 6}" '
             f'stroke="{paper}" stroke-width="{stem_w}" fill="none" stroke-linecap="round" stroke-linejoin="round"/>')
    pills = "".join(f'<rect x="{a}" y="{b}" width="{c - a}" height="{d - b}" rx="{(d - b) / 2}" fill="{col}"/>' for a, b, c, d, col in nodes)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="{px}" height="{px}">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>
    <radialGradient id="shine" cx=".28" cy=".18" r=".85"><stop offset="0" stop-color="#fff" stop-opacity=".32"/><stop offset=".6" stop-color="#fff" stop-opacity="0"/></radialGradient>
  </defs>
  <rect x="6" y="6" width="244" height="244" rx="{50 if small else 58}" fill="url(#bg)"/>
  <rect x="6" y="6" width="244" height="244" rx="{50 if small else 58}" fill="url(#shine)"/>
  <rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{y1 - y0}" rx="{rr}" fill="{paper}"/>
  {branches}
  {elbow}
  {pills}
</svg>"""


def render(markup: str, px: int, out: pathlib.Path) -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        page = pathlib.Path(tmp) / "icon.html"
        page.write_text(f'<html><body style="margin:0;background:transparent">{markup}</body></html>', encoding="utf-8")
        shot = pathlib.Path(tmp) / "shot.png"
        side = max(300, px + 20)
        subprocess.run([BROWSER, "--headless=new", f"--user-data-dir={pathlib.Path(tmp) / 'profile'}", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
                        "--default-background-color=00000000", f"--window-size={side},{side}",
                        f"--screenshot={shot}", page.as_uri()], check=True, capture_output=True, timeout=60)
        img = Image.open(shot).convert("RGBA").crop((0, 0, px, px))
        img.load()
    img.save(out)
    return img


def main():
    out = ROOT / "art" / "build"
    out.mkdir(parents=True, exist_ok=True)
    for paused, name in [(False, "twig"), (True, "twig-paused")]:
        imgs = [render(svg(s, paused), s, out / f"{name}-{s}.png") for s in SIZES]
        imgs[-1].save(ROOT / "res" / f"{name}.ico", sizes=[(s, s) for s in SIZES], append_images=imgs[:-1])
        (out / f"{name}.svg").write_text(svg(256, paused), encoding="utf-8")
    render(svg(512, False), 512, ROOT / "art" / "twig-icon-512.png")
    # Raw RGBA for the settings window's title-bar icon (no image decoder needed at runtime).
    (ROOT / "res" / "icon-64.rgba").write_bytes(Image.open(out / "twig-64.png").convert("RGBA").tobytes())
    print("icons written")


if __name__ == "__main__":
    main()
