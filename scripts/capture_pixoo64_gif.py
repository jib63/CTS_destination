#!/usr/bin/env python3
"""Refresh README images from the actual Rust Pixoo64 renderer, offline.

Requires Pillow: python3 -m pip install pillow
The GIF accelerates a 15-minute approach; the tram position is never GPS data.
"""

import json
import os
import subprocess
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
FRAMES = ROOT / "target" / "pixoo64-docs"
OUTPUT = ROOT / "docs" / "screenshots"


def scaled(name):
    with Image.open(FRAMES / f"{name}.png") as source:
        assert source.size == (64, 64), "Expected an actual 64×64 device frame"
        return source.convert("RGB").resize((384, 384), Image.Resampling.NEAREST)


def main():
    env = dict(os.environ, TZ="Europe/Paris")
    subprocess.run(
        ["cargo", "run", "--offline", "--locked", "--example", "render_pixoo64", "--", str(FRAMES)],
        cwd=ROOT, env=env, check=True,
    )
    OUTPUT.mkdir(parents=True, exist_ok=True)
    stills = {
        "station-4-lines": "pixoo64-station.png",
        "station-2-lines": "pixoo64-station-2-lines.png",
        "station-birthday": "pixoo64-station-birthday.png",
        "station-countdown": "pixoo64-station-countdown.png",
    }
    for source, target in stills.items():
        scaled(source).save(OUTPUT / target)
    manifest = json.loads((FRAMES / "frames.json").read_text())
    images = [scaled(item["name"]) for item in manifest]
    images[0].save(
        OUTPUT / "pixoo64.gif", save_all=True, append_images=images[1:],
        duration=[item["duration_ms"] for item in manifest], loop=0,
        optimize=True, disposal=2,
    )
    print(f"Saved {len(stills)} PNGs and {len(images)} GIF frames to {OUTPUT}")


if __name__ == "__main__":
    main()
