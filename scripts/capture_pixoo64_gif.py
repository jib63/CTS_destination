#!/usr/bin/env /Library/Frameworks/Python.framework/Versions/3.13/bin/python3
"""Capture an animated GIF of the Pixoo64 mockup using Chrome CDP."""

import asyncio, base64, json, subprocess, sys, time, urllib.request
from io import BytesIO
from pathlib import Path
import PIL.Image, PIL.ImageSequence
import websockets

OUT_GIF  = Path(__file__).parent.parent / "docs/screenshots/pixoo64.gif"
CHROME   = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
PORT     = 9224
HTTP_PORT = 8765           # python -m http.server started in project root
WIN_W, WIN_H = 960, 660

# Capture 18 s at 4 fps → 72 raw frames; keep every 2nd for GIF (2 fps ≈ 500 ms/frame)
CAPTURE_SECS = 18
CAPTURE_FPS  = 4
GIF_KEEP_NTH = 2      # keep 1 in N frames
GIF_FRAME_MS = 500    # ms per GIF frame


async def cdp(ws, method, params=None, _id_counter=[0]):
    _id_counter[0] += 1
    cmd_id = _id_counter[0]
    await ws.send(json.dumps({"id": cmd_id, "method": method, **({"params": params} if params else {})}))
    while True:
        msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=10))
        if msg.get("id") == cmd_id:
            if "error" in msg:
                raise RuntimeError(f"CDP error: {msg['error']}")
            return msg.get("result", {})


async def capture():
    print(f"Starting Chrome headless on port {PORT}…")
    proc = subprocess.Popen(
        [CHROME, f"--remote-debugging-port={PORT}", "--headless=new",
         f"--window-size={WIN_W},{WIN_H}", "--disable-gpu", "--no-sandbox",
         "--disable-extensions", "--disable-background-networking"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    await asyncio.sleep(2.0)

    try:
        raw  = urllib.request.urlopen(f"http://localhost:{PORT}/json", timeout=5).read()
        tabs = json.loads(raw)
        if not tabs:
            raise RuntimeError("No Chrome tabs found")
        page_tab = next((t for t in tabs if t.get("type") == "page"), tabs[0])
        ws_url = page_tab["webSocketDebuggerUrl"]
        print(f"  Connected to {ws_url[:60]}…")

        frames = []
        async with websockets.connect(ws_url, max_size=20_000_000) as ws:
            await cdp(ws, "Page.enable")
            await cdp(ws, "Emulation.setDeviceMetricsOverride",
                      {"width": WIN_W, "height": WIN_H, "deviceScaleFactor": 1, "mobile": False})

            url = f"http://localhost:{HTTP_PORT}/mockup_pixoo64.html"
            print(f"  Navigating to {url}…")
            await cdp(ws, "Page.navigate", {"url": url})
            # Wait for loadEventFired then let JS initialise
            for _ in range(60):
                try:
                    msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=0.5))
                    if msg.get("method") == "Page.loadEventFired":
                        break
                except asyncio.TimeoutError:
                    pass
            await asyncio.sleep(2.0)  # let first frame render

            # Locate canvas-wrap bounding box
            result = await cdp(ws, "Runtime.evaluate", {"expression": """
                (function(){
                  const el = document.getElementById('canvas-wrap');
                  const r  = el.getBoundingClientRect();
                  return JSON.stringify({x:Math.round(r.left), y:Math.round(r.top),
                                         w:Math.round(r.width), h:Math.round(r.height)});
                })()
            """})
            bounds = json.loads(result["result"]["value"])
            print(f"  Canvas-wrap bounds: {bounds}")

            total = CAPTURE_SECS * CAPTURE_FPS
            interval = 1.0 / CAPTURE_FPS
            print(f"  Capturing {total} frames over {CAPTURE_SECS} s…")

            for i in range(total):
                res = await cdp(ws, "Page.captureScreenshot", {
                    "format": "png",
                    "clip": {"x": bounds["x"], "y": bounds["y"],
                             "width": bounds["w"], "height": bounds["h"], "scale": 1},
                })
                raw_png = base64.b64decode(res["data"])
                img = PIL.Image.open(BytesIO(raw_png)).convert("RGB")
                # Scale down to 270 × 270 for a compact GIF
                img = img.resize((270, 270), PIL.Image.LANCZOS)
                frames.append(img)
                print(f"  Frame {i+1:02d}/{total}", end="\r", flush=True)
                await asyncio.sleep(interval)

            print()
        return frames

    finally:
        proc.terminate()
        proc.wait()


def save_gif(frames):
    keep = frames[::GIF_KEEP_NTH]
    OUT_GIF.parent.mkdir(parents=True, exist_ok=True)

    # Build a global palette from all frames combined, then remap each frame
    all_pixels = PIL.Image.new("RGB", (keep[0].width, keep[0].height * len(keep)))
    for i, f in enumerate(keep):
        all_pixels.paste(f, (0, i * keep[0].height))
    global_pal = all_pixels.quantize(colors=200, method=PIL.Image.Quantize.MEDIANCUT)

    palette_frames = [f.quantize(palette=global_pal) for f in keep]

    palette_frames[0].save(
        OUT_GIF,
        save_all=True,
        append_images=palette_frames[1:],
        loop=0,
        duration=[GIF_FRAME_MS] * len(palette_frames),
        optimize=False,
    )
    size = OUT_GIF.stat().st_size
    print(f"Saved {len(palette_frames)} frames → {OUT_GIF}")
    print(f"File size: {size / 1024:.0f} KB")


def main():
    frames = asyncio.run(capture())
    if not frames:
        print("No frames captured!", file=sys.stderr)
        return 1
    save_gif(frames)
    return 0


if __name__ == "__main__":
    sys.exit(main())
