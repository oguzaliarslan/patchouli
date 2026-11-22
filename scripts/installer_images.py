"""Draws the installer artwork into src-tauri/installer/.

NSIS: sidebar.bmp (164x314, welcome/finish pages), header.bmp (150x57).
WiX (MSI): banner.bmp (493x58), dialog.bmp (493x312).
Installers need 24-bit BMPs at exactly these sizes. Everything is drawn at
3x and scaled down for smooth edges. Run from the project root:

    python scripts/installer_images.py
"""

import math
import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
ICON = ROOT / "src-tauri" / "icons" / "icon.png"
OUT = ROOT / "src-tauri" / "installer"
FONTS = Path("C:/Windows/Fonts")
SCALE = 3

# From the app icon.
NIGHT_TOP = (24, 16, 52)
NIGHT_MID = (42, 28, 86)
NIGHT_LOW = (70, 48, 140)
VIOLET = (124, 86, 246)
GOLD = (245, 215, 126)
LAVENDER = (196, 184, 238)
WHITE = (255, 255, 255)


def font(name: str, size: int) -> ImageFont.FreeTypeFont:
    try:
        return ImageFont.truetype(str(FONTS / name), size * SCALE)
    except OSError:
        return ImageFont.load_default(size * SCALE)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def night(width: int, height: int, seed: int, keep_clear=None) -> Image.Image:
    """A vertical night-sky gradient with a glow low down and a few stars.
    No stars inside `keep_clear`, a box in unscaled px (x0, y0, x1, y1)."""
    w, h = width * SCALE, height * SCALE
    sky = Image.new("RGB", (w, h))
    px = sky.load()
    for y in range(h):
        t = y / max(1, h - 1)
        colour = lerp(NIGHT_TOP, NIGHT_MID, t / 0.6) if t < 0.6 else lerp(NIGHT_MID, NIGHT_LOW, (t - 0.6) / 0.4)
        for x in range(w):
            px[x, y] = colour

    glow = Image.new("L", (w, h), 0)
    ImageDraw.Draw(glow).ellipse((-w * 0.4, h * 0.62, w * 1.4, h * 1.5), fill=120)
    glow = glow.filter(ImageFilter.GaussianBlur(w * 0.18))
    sky = Image.composite(Image.new("RGB", (w, h), VIOLET), sky, glow)

    rng = random.Random(seed)
    stars = ImageDraw.Draw(sky)
    clear = tuple(v * SCALE for v in keep_clear) if keep_clear else None

    def blocked(x, y):
        return clear and clear[0] <= x <= clear[2] and clear[1] <= y <= clear[3]

    for _ in range(int(width * height / 560)):
        x, y = rng.uniform(0, w), rng.uniform(0, h * 0.7)
        if blocked(x, y):
            continue
        r = rng.choice([0.6, 0.8, 1.0, 1.4]) * SCALE
        shade = lerp(LAVENDER, WHITE, rng.random())
        stars.ellipse((x - r, y - r, x + r, y + r), fill=shade)
    # A couple of brighter four-point sparkles.
    for _ in range(3):
        x, y = rng.uniform(w * 0.15, w * 0.85), rng.uniform(h * 0.05, h * 0.45)
        if blocked(x, y):
            continue
        s = rng.uniform(3, 5) * SCALE
        stars.polygon([(x, y - s), (x + s / 4, y), (x, y + s), (x - s / 4, y)], fill=GOLD)
        stars.polygon([(x - s, y), (x, y - s / 4), (x + s, y), (x, y + s / 4)], fill=GOLD)
    return sky


def icon(size: int, shadow: bool = True) -> Image.Image:
    """The app icon at `size` (unscaled px), with a soft drop shadow."""
    s = size * SCALE
    art = Image.open(ICON).convert("RGBA").resize((s, s), Image.LANCZOS)
    if not shadow:
        return art
    pad = s // 5
    canvas = Image.new("RGBA", (s + pad * 2, s + pad * 2), (0, 0, 0, 0))
    alpha = art.getchannel("A").point(lambda a: a * 0.55)
    shade = Image.new("RGBA", art.size, (8, 4, 24, 0))
    shade.putalpha(alpha)
    canvas.alpha_composite(shade, (pad, pad + s // 18))
    canvas = canvas.filter(ImageFilter.GaussianBlur(s / 22))
    canvas.alpha_composite(art, (pad, pad))
    return canvas


def paste_centered(base: Image.Image, overlay: Image.Image, cx: float, cy: float) -> None:
    base.paste(overlay, (round(cx - overlay.width / 2), round(cy - overlay.height / 2)), overlay)


def text_centered(draw, cx, y, text, fnt, fill):
    width = draw.textlength(text, font=fnt)
    draw.text((cx - width / 2, y), text, font=fnt, fill=fill)


def finish(image: Image.Image, width: int, height: int, name: str) -> None:
    out = image.convert("RGB").resize((width, height), Image.LANCZOS)
    OUT.mkdir(parents=True, exist_ok=True)
    out.save(OUT / name, format="BMP")
    print(f"{name}: {out.size[0]}x{out.size[1]}")


def art_panel(width: int, height: int, seed: int, icon_size: int, title_size: int) -> Image.Image:
    """The night panel with the icon, name and tagline: the NSIS sidebar,
    and the left of the MSI welcome dialog."""
    # Keep the icon, name and tagline clear of stars.
    top = height * 0.36 - icon_size * 0.62
    bottom = height * 0.36 + icon_size * 0.62 + title_size * 2.4
    panel = night(width, height, seed, keep_clear=(8, top, width - 8, bottom))
    cx = panel.width / 2
    paste_centered(panel, icon(icon_size), cx, panel.height * 0.36)
    draw = ImageDraw.Draw(panel)
    y = panel.height * 0.36 + icon_size * SCALE * 0.62
    text_centered(draw, cx, y, "Patchouli", font("segoeuib.ttf", title_size), WHITE)
    text_centered(draw, cx, y + title_size * SCALE * 1.35, "Anime tracker", font("segoeui.ttf", round(title_size * 0.55)), LAVENDER)
    # Thin gold rule above the bottom edge.
    rule_y = panel.height - 22 * SCALE
    draw.line((cx - 18 * SCALE, rule_y, cx + 18 * SCALE, rule_y), fill=GOLD, width=SCALE)
    return panel


def sidebar():
    finish(art_panel(164, 314, seed=7, icon_size=84, title_size=19), 164, 314, "sidebar.bmp")


def header():
    # NSIS shows this at the right of a white header bar.
    w, h = 150 * SCALE, 57 * SCALE
    image = Image.new("RGB", (w, h), WHITE)
    fade = Image.new("L", (w, h), 0)
    fpx = fade.load()
    for x in range(w):
        a = int(70 * max(0.0, (x / w - 0.35) / 0.65) ** 1.6)
        for y in range(h):
            fpx[x, y] = a
    image = Image.composite(Image.new("RGB", (w, h), LAVENDER), image, fade)
    paste_centered(image, icon(40, shadow=False), w - 30 * SCALE, h / 2)
    draw = ImageDraw.Draw(image)
    f = font("segoeuib.ttf", 13)
    draw.text((w - 56 * SCALE - draw.textlength("Patchouli", font=f), h / 2 - 9 * SCALE), "Patchouli", font=f, fill=(58, 39, 112))
    finish(image, 150, 57, "header.bmp")


def banner():
    # WiX writes the dialog title in black at the top left, so keep that light.
    w, h = 493 * SCALE, 58 * SCALE
    image = Image.new("RGB", (w, h), WHITE)
    strip = night(493, 58, seed=11)
    mask = Image.new("L", (w, h), 0)
    mpx = mask.load()
    for x in range(w):
        t = max(0.0, (x / w - 0.55) / 0.45)
        a = int(255 * (0.5 - 0.5 * math.cos(math.pi * min(1.0, t * 1.4))))
        for y in range(h):
            mpx[x, y] = a
    image = Image.composite(strip, image, mask)
    paste_centered(image, icon(40, shadow=False), w - 32 * SCALE, h / 2)
    finish(image, 493, 58, "banner.bmp")


def dialog():
    # Welcome and finish dialogs: art on the left, text on the white right.
    w, h = 493 * SCALE, 312 * SCALE
    image = Image.new("RGB", (w, h), WHITE)
    panel = art_panel(164, 312, seed=5, icon_size=84, title_size=19)
    image.paste(panel, (0, 0))
    draw = ImageDraw.Draw(image)
    draw.line((panel.width, 0, panel.width, h), fill=LAVENDER, width=SCALE)
    finish(image, 493, 312, "dialog.bmp")


if __name__ == "__main__":
    sidebar()
    header()
    banner()
    dialog()
