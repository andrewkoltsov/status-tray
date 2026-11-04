#!/usr/bin/env python3
"""Generate tray icons and the Tauri app icon set (needs Pillow).

    pip install pillow
    python3 scripts/gen_icons.py

Writes src-tauri/icons/tray/{ok,minor,major,unknown}-{32,64}.png and the bundle icons
(32x32.png, 128x128.png, 128x128@2x.png, icon.png, icon.ico, icon.icns).
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "src-tauri" / "icons"
TRAY = ICONS / "tray"

COLORS = {
    "ok": (46, 160, 67),
    "minor": (230, 170, 0),
    "major": (215, 58, 73),
    "unknown": (140, 145, 150),
}
SS = 8  # supersampling factor for smooth edges


def circle(size: int, color) -> Image.Image:
    big = size * SS
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    pad = big // 8
    d.ellipse((pad, pad, big - pad - 1, big - pad - 1), fill=color + (255,))
    return img.resize((size, size), Image.LANCZOS)


def app_icon(size: int = 1024) -> Image.Image:
    """Dark rounded square with a green status dot."""
    big = size * 2
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, big - 1, big - 1), radius=big // 5, fill=(32, 36, 44, 255))
    m = big // 4
    d.ellipse((m, m, big - m - 1, big - m - 1), fill=COLORS["ok"] + (255,))
    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    TRAY.mkdir(parents=True, exist_ok=True)
    for name, color in COLORS.items():
        for size in (32, 64):
            circle(size, color).save(TRAY / f"{name}-{size}.png")

    master = app_icon(1024)
    for filename, size in {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }.items():
        master.resize((size, size), Image.LANCZOS).save(ICONS / filename)
    master.save(ICONS / "icon.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    master.save(ICONS / "icon.icns")
    print("icons written to", ICONS)


if __name__ == "__main__":
    main()
