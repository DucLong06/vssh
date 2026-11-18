#!/usr/bin/env python3
"""Render the vssh brand icon (vsh-final2.svg) into the full icon set.

Outputs (in docs/brand/final/):
  icon-1024/512/256/128/64/48/32/16.png
  vssh-lockup.png (icon + wordmark, for READMEs)
"""
import subprocess, sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
RESAMPLE = getattr(getattr(Image, "Resampling", Image), "LANCZOS")

BRAND = Path("/mnt/data/Project/vssh/docs/brand")
OUT = BRAND / "final"
OUT.mkdir(parents=True, exist_ok=True)
SVG = BRAND / "vsh-final2.svg"
SHEET = BRAND / "lockup.svg"

def render(svg: Path, size: int, out: Path):
    subprocess.run([
        "google-chrome", "--headless", "--disable-gpu",
        f"--window-size={size},{size}",
        f"--screenshot={out}", f"file://{svg}",
    ], check=True, capture_output=True, timeout=60)

# 1. all square sizes from the master SVG
big = OUT / "icon-1024.png"
render(SVG, 1024, big)
im = Image.open(big).convert("RGBA")
for s in (512, 256, 128, 64, 48, 32, 16):
    im.resize((s, s), RESAMPLE).save(OUT / f"icon-{s}.png")

# 2. horizontal lockup: icon + "vssh" wordmark on the dark tile colour
lockup = Image.new("RGBA", (2048, 512), (22, 22, 30, 255))  # #16161e
icon = Image.open(OUT / "icon-512.png")
lockup.paste(icon, (64, 0), icon)
try:
    font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 300)
except Exception:
    font = ImageFont.load_default()
d = ImageDraw.Draw(lockup)
d.text((680, 96), "vssh", font=font, fill=(122, 162, 247, 255))  # #7aa2f7
lockup.save(OUT / "lockup-1600x400.png")

print("sizes:", sorted(p.name for p in OUT.iterdir()))
