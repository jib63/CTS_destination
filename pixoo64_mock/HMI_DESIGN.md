# Pixoo64 HMI Design — Ambient Hub

The Pixoo64 renderer displays a 64×64 pixel LED matrix connected via HTTP.
The design is called the **Ambient Hub**: a permanent header shows the current time
and weather, and the body cycles through departure stops and moment screens.

---

## 1. Screen anatomy

Every screen shares the same header band (y=0..28). Only the body (y=29..63) differs.

```
┌────────────────────────────────────────────────────────────────┐  y=0
│  17:42          ☁  14°  8/19°                                  │  ← header (29px)
├────────────────────────────────────────────────────────────────┤  y=28 (1px divider)
│                                                                │
│            [ body — hub or moment ]                            │  ← body (35px)
│                                                                │
└────────────────────────────────────────────────────────────────┘  y=63
```

### 1.1 Header band (y=0..27)

| Element       | Position              | Color / scale |
|---------------|-----------------------|---------------|
| Clock HH:MM   | x=2, y=2, scale 2     | WHITE (#FFFFFF) |
| Weather icon  | x=2, y=20 (6×5 glyph) | — |
| Temp now      | x=10, y=19            | YELLOW (#FFDC50) |
| Min/Max temp  | x=32, y=19            | TEXT_MID (#8FA0BB) |
| No weather    | "--/--°" at x=32, y=19 | TEXT_DIM (#5E6F8E) |

Divider: 1px line at y=28, color DIVIDER (#1E3250).

---

## 2. Hub screen (departure board)

The hub screen shows one stop's departures as full-width color-block rows.
Density is controlled by `pixoo64_lines_per_screen` (1–4 rows).

### 2.1 Row geometry

| Lines | Stop name | Row height | Start y   |
|-------|-----------|------------|-----------|
| 4     | hidden    | 8px        | 30        |
| 3     | y=30      | 8px        | 39        |
| 2     | y=30      | 13px       | 38 / 51   |
| 1     | y=30      | 18px       | 42        |

In 4-line mode, the bottom 2px (y=62..63) are filled with BG_NAVY to avoid a cut edge.

Stop name: up to 10 characters, color TEXT_DIM, scale 1.

### 2.2 Color-block row

Each row spans the full 64px width. The background is the line's brand color tinted
toward navy using the formula: `result = brand * 0.42 + (26,38,66) * 0.58`.

```
┌──────────────────────────────────────────────────────────────┐  y
│  A   ILLK                          3        15               │
└──────────────────────────────────────────────────────────────┘  y+h
  x=2  x=10                        x=49     x=63
```

| Element      | x             | Color                              |
|--------------|---------------|------------------------------------|
| Line letter  | 2             | Full brand                         |
| Destination  | 10 (4 chars)  | WHITE                              |
| Next time    | right @ x=49  | YELLOW, or HOT_ORANGE if < 2 min   |
| After time   | right @ x=63  | AMBER_DIM                          |

The destination is truncated to 4 chars, uppercased, from `destination_short`.

Next departure time color: YELLOW `#FFDC50` normally; HOT_ORANGE `#FF8C1E` (255, 140, 30)
when `minutes_remaining < 2` — signals imminent departure without an aggressive red.

### 2.3 Line brand colors

| Line | RGB           |
|------|---------------|
| A    | 200, 16, 46   |
| B    | 123, 45, 139  |
| C    | 224, 123, 16  |
| D    | 46, 139, 60   |
| E    | 111, 44, 145  |
| F    | 197, 20, 91   |
| G    | 92, 107, 192  |
| —    | 84, 110, 122  |

### 2.4 "No data" fallback

If the board has no departure lines (offline or no data), the text "no data" is
centered horizontally at y=42 in TEXT_DIM color.

---

## 3. Moment screen

Compact screens inserted between stop passes to show birthdays and Jour J events.

```
┌────────────────────────────────────────────────────────────────┐  y=0
│                   17:42                                        │  ← compact clock (y=1)
├────────────────────────────────────────────────────────────────┤  y=9 (divider)
│                                                                │
│   [icon]  J-15  ←── hero band (y=11..28, 18px, tinted)        │
│                                                                │
├────────────────────────────────────────────────────────────────┤  y=29
│  Anniversaire                                                  │
│  Mariage                                                       │  ← body text
│                                                                │
└────────────────────────────────────────────────────────────────┘  y=63
```

### 3.1 Compact header (y=0..9)

The clock `HH:MM` (5 chars × 6px = 30px) is centered: x=17, y=1, scale 1, TEXT_MID.
Divider at y=9.

### 3.2 Hero band (y=11..28)

Background: tint_block applied to the moment's brand color.

**Birthday** (prefix = ""): cake icon centered at x=29, y=17.

**Jour J event** (prefix = "J-N"): icon at x=4, y=17; prefix text at x=14, y=13, scale 2, in brand color.

### 3.3 Body text (y=31+)

The event label is broken into chunks of 10 characters (letter boundary, no word wrap),
up to 4 lines at y=31, 40, 49, 58 (9px spacing), scale 1, WHITE.

### 3.4 Moment icons (6×7 px, placed at x, y)

| Icon key | Icon used     | Color       |
|----------|---------------|-------------|
| cake     | Birthday cake | CAKE_GOLD   |
| present  | Gift box      | GIFT_BLUE   |
| party    | Gift box      | GIFT_BLUE   |
| heart    | Heart         | HEART_LIL   |
| star     | Star          | STAR_GOLD   |
| skull    | Star          | STAR_GOLD   |

---

## 4. Rotation cycle

```
[Hub · Stop 1] → [Hub · Stop 2] → … → [Moment 1] → … → [Hub · Stop 1] → …
```

- Stops with no departure lines **and** no offline message are skipped.
- If all stops are empty, one Hub slot for stop 0 is always kept.
- Moment screens follow all Hub screens.
- If there are no moments, the cycle is Hub screens only.

### 4.1 Cycle timing

| Slot type | Duration config key              | Default | Range   |
|-----------|----------------------------------|---------|---------|
| Hub       | `pixoo64_tram_screen_seconds`    | 6 s     | 1..60   |
| Moment    | `pixoo64_moment_screen_seconds`  | 1 s     | 1..30   |

### 4.2 Re-render triggers

A new frame is sent to the device only when:
1. The current slot's duration expires (screen rotation).
2. New departure data arrives from the poll loop.
3. The wall-clock minute changes (clock display update).

Always exactly **1 static frame** — no animation.

---

## 5. Color palette

| Name       | RGB           | Hex     | Use |
|------------|---------------|---------|-----|
| BG_NAVY    | 14, 20, 40    | #0E1428 | Background |
| DIVIDER    | 30, 50, 80    | #1E3250 | Separator lines |
| WHITE      | 255, 255, 255 | #FFFFFF | Destination, body text |
| YELLOW     | 255, 220, 80  | #FFDC50 | Next departure time |
| AMBER_DIM  | 160, 122, 40  | #A07A28 | After departure time |
| TEXT_DIM   | 94, 111, 142  | #5E6F8E | Stop name, labels |
| TEXT_MID   | 143, 160, 187 | #8FA0BB | Weather strip, compact clock |
| CAKE_GOLD  | 255, 224, 144 | #FFE090 | Birthday cake icon |
| GIFT_BLUE  | 180, 200, 255 | #B4C8FF | Present icon |
| HEART_LIL  | 220, 171, 255 | #DCABFF | Heart icon |
| STAR_GOLD  | 255, 210, 63  | #FFD23F | Star icon |

---

## 6. Configuration keys

| Key                              | Type | Default | Range  | Hot-reload |
|----------------------------------|------|---------|--------|------------|
| `pixoo64_enabled`                | bool | false   | —      | No         |
| `pixoo64_address`                | str  | —       | —      | No         |
| `pixoo64_simulation`             | bool | false   | —      | No         |
| `pixoo64_brightness`             | u8   | —       | 0..100 | No (startup) |
| `pixoo64_tram_screen_seconds`    | u32  | 6       | 1..60  | Yes (POST /api/config) |
| `pixoo64_moment_screen_seconds`  | u32  | 1       | 1..30  | Yes |
| `pixoo64_lines_per_screen`       | u8   | 4       | 1..4   | Yes |

Hot-reload via `POST /api/config` with JSON body containing the new values.
Changes take effect on the next rendered frame.

---

## 7. Rendering pipeline

```
BoardPayload (departures + weather + moments)
    │
    ▼  sent by WebSocket poll loop on each data refresh
UnboundedChannel<Box<BoardPayload>>
    │
    ▼  pixoo_worker (async task)
    ├─ extract_moments(payload) → Vec<MomentItem>
    ├─ build_cycle(payload, moments, tram_secs, moment_secs) → Vec<ScreenSlot>
    │
    ├─ on data: reset elapsed, re-render current slot
    ├─ on 1-second tick:
    │    elapsed += 1
    │    if elapsed >= slot.duration → advance pos, render new slot
    │    elif minute changed → re-render (clock update)
    │
    ├─ render_hub_frame(fb, board, lines_per_screen) → base64 RGB
    │   or render_moment_frame(fb, kind, prefix, body, color) → base64 RGB
    │
    ├─ fb_to_png(fb) → PNG bytes → stored in AppState.pixoo64_preview
    │   (served at GET /api/pixoo64/preview in the web UI)
    │
    └─ HTTP POST Draw/SendHttpGif to pixoo64_address (if not simulation)
```

---

## 8. Preview endpoint

`GET /api/pixoo64/preview` returns the last rendered frame as a 64×64 PNG.
Returns 204 No Content if no frame has been rendered yet.
Returns 404 if `pixoo64_enabled = false`.

The web UI's Pixoo64 tab fetches this endpoint periodically to show a live preview.

---

## 9. Interactive mockup

`mockup_pixoo64.html` is a self-contained browser mockup that mirrors the Rust
drawing logic in JavaScript. It renders on an 8× scaled canvas (512×512 px) and
provides:
- Editable stop data (name, departure lines, next/after times)
- Weather controls (pictocode, temperatures)
- Moment controls (birthday names, Jour J events with icon picker)
- Three sliders: tram_secs, moment_secs, lines_per_screen
- Rotation cycle bar with per-slot progress
- Play/Pause and Prev/Next navigation
