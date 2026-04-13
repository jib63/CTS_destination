# Architecture — CTS Departures

A single-binary Rust web application that polls the CTS (Compagnie des Transports Strasbourgeois) SIRI 2.0 API and serves a live departure board over WebSocket to connected browsers. An optional Meteoblue weather widget is displayed in the board footer. An optional Divoom Pixoo64 renderer drives a 64×64 LED matrix display.

## High-level overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                             main.rs                                  │
│  load config → build AppState → spawn CTS poll task                  │
│                              → spawn weather poll task (optional)    │
│                              → start Axum web server                 │
└──────────────────────────┬───────────────────────────────────────────┘
                           │  Arc<AppState>
         ┌─────────────────┼─────────────────┐
         ▼                 ▼                 ▼
┌────────────────┐  ┌─────────────┐  ┌───────────────────────┐
│  CTS poll task │  │ Weather     │  │   Axum web server     │
│  cts/client.rs │  │ poll task   │  │   web/router.rs       │
│                │  │ meteoblue/  │  │                       │
│  fetch_departs │  │ client.rs   │  │  GET  /ws             │
│  simulate_board│  │             │  │  GET  /api/stops      │
│  offline_board │  │  resolve    │  │  GET  /api/stops/     │
│       │        │  │  coords     │  │       :code/details   │
│       │        │  │  fetch      │  │  POST /api/config     │
│       ▼        │  │  weather    │  │  POST /api/jour-j     │
│  DisplayRenderer│  │     │      │  │  GET  /api/status     │
│  ┌──────────┐  │  │     │      │  │  GET  /api/pixoo64/   │
│  │WebRenderer│  │◄─┘  patches  │  │       preview         │
│  │(broadcast)│  │  latest +    │  │                       │
│  └──────────┘  │  rebroadcast  │  │  ► WS clients         │
│  ┌──────────┐  │  └────────────┘  └───────────────────────┘
│  │Pixoo64   │  │
│  │Renderer  │  │
│  └──────────┘  │
└────────────────┘
```

## Module map

```
src/
├── main.rs               Entry point, lifecycle, graceful shutdown
├── config.rs             TOML config loading and in-place persistence
│
├── cts/
│   ├── mod.rs
│   ├── client.rs         CTS API client, poll loop, stop discovery
│   ├── model.rs          SIRI JSON data structures + ISO duration helper
│   └── simulation.rs     Fake departure data generator (cts_simulation = true)
│
├── departure/
│   ├── mod.rs
│   └── model.rs          API-agnostic DepartureBoard domain model;
│                         birthday loading; Jour J countdown computation
│
├── display/
│   └── mod.rs            DisplayRenderer trait
│
├── meteoblue/
│   ├── mod.rs            Re-exports client, model, simulation
│   ├── client.rs         Location resolution (query3 API) + weather poll loop
│   ├── model.rs          MeteoblueResponse, WeatherCoords, WeatherSnapshot
│   └── simulation.rs     Fixed offline WeatherSnapshot (Strasbourg values)
│
├── pixoo64/
│   ├── mod.rs
│   ├── draw.rs           Frame composition (departures, weather, birthday, Jour J)
│   ├── font.rs           Embedded 5×7 pixel bitmap font
│   └── renderer.rs       Pixoo64 HTTP client and DisplayRenderer impl
│
└── web/
    ├── mod.rs            AppState definition; CronMatcher + crontab parser
    ├── router.rs         Axum router, REST handlers, embedded static files
    └── ws.rs             WebSocket connection handling
```

## Data flow

### Startup

1. `main.rs` reads `config.toml` (or a path from `argv[1]`).
2. `AppState` is constructed: HTTP client, broadcast channel, `RwLock`s for mutable config, parsed `CronMatcher` lists.
3. A Tokio task is spawned for the CTS poll loop.
4. If `meteoblue_enabled`, a second Tokio task is spawned for the weather poll loop. It resolves the city name to coordinates once at startup, then polls on a configurable interval.
5. If `pixoo64_enabled`, a `Pixoo64Renderer` is registered with the poll loop's renderer list.
6. The Axum web server is started; all tasks share the same `Arc<AppState>`.

### CTS poll loop (`cts/client::poll_loop`)

```
loop:
  sleep until next_poll  (or wake early via poll_trigger Notify)
  read monitoring_ref    (RwLock)

  if not cts_always_query AND outside crontab window:
      push offline DepartureBoard → renderers
      sleep 60 s, recheck

  elif cts_simulation:
      cts/simulation::simulate_board(demo_lines)  → renderers

  else:
      cts/client::fetch_departures()  → renderers
      clamp interval to API's ShortestPossibleCycle

  if birthday_enabled:
      board.birthdays_today = DepartureBoard::load_birthdays(file)

  if jour_j_enabled AND jour_j_date is set:
      board.jour_j = DepartureBoard::compute_jour_j(date, label)

  if weather_enabled:
      board.weather = meteoblue_latest.read()   (cached by weather task)

  schedule next_poll, store unix timestamp in AtomicI64
```

### Weather poll loop (`meteoblue/client::weather_poll_loop`)

```
startup:
  resolve city name → (lat, lon, asl) via Meteoblue query3 API
  store in AppState::meteoblue_coords

loop:
  if not meteoblue_always_query AND outside crontab window:
      sleep 60 s, recheck

  if meteoblue_simulation:
      snap = simulation::simulate_weather()
  else:
      fetch Meteoblue basic-1h_basic-day API
      snap = WeatherSnapshot::from_response()

  store_and_rebroadcast(snap):
      meteoblue_latest.write() = snap
      patch latest cached board JSON: board["weather"] = snap
      broadcast patched JSON to all WS clients

  sleep meteoblue_polling_interval_minutes * 60 s
  (on error: log warning, retry after 5 minutes)
```

The `store_and_rebroadcast` step ensures the weather footer updates immediately even when the weather poll fires between two CTS polls.

### Pixoo64 renderer (`pixoo64/renderer.rs`, `pixoo64/draw.rs`)

```
on render(board):
  draw_frame(board):
      draw header:  stop name + clock
      draw rows:    up to 4 departure rows
                    each row: line badge | scrolling destination | next time | following time
                    real-time times in bold; theoretical in italic (colour-coded)
      if rows < 4 AND birthday_enabled AND birthdays_today non-empty:
          draw separator (1 blank + 1 gray + 1 blank pixel)
          draw birthday row: present icon + scrolling "Name (age)" text
      if rows < 3 AND jour_j_enabled AND jour_j set:
          draw Jour J row: party icon + "J-N" badge + scrolling event label
      draw weather footer (when weather available):
          pictocode icon | temperature | precipitation | sunshine

  if pixoo64_simulation:
      encode frame as PNG → store in AppState::pixoo64_preview
  else:
      POST frame to device HTTP API
```

Scrolling state (`dest_scroll[i]`) is maintained per row across ticks. Text wider than the destination area scrolls pixel-by-pixel, resetting with a brief pause.

### WebSocket connection (`web/ws.rs`)

```
client connects
  → subscribe to broadcast channel   (avoids startup race)
  → send cached `latest` snapshot    (immediate paint, includes weather)
  → relay every broadcast message to client
  → detect close / lagged receiver
```

### Config hot-reload

**Stop change** (`POST /api/config`):
```
validate new cts_monitoring_ref
→ save_monitoring_ref()   rewrites line in config.toml (preserves comments)
→ state.monitoring_ref.write()
→ state.poll_trigger.notify_one()   wakes CTS poll loop immediately
```

**Jour J update** (`POST /api/jour-j`):
```
validate date format (DD/MM/YYYY)
→ save_jour_j()   rewrites/appends jour_j_date and jour_j_label in config.toml
→ state.jour_j_date.write(), state.jour_j_label.write()
```

## Crontab query windows

Both the CTS poll loop and the Meteoblue poll loop can be gated to specific time windows using 5-field crontab expressions (`min hour dom month dow`). This allows different schedules per day-of-week.

**Struct** (`web/mod.rs`):
```rust
pub struct CronMatcher {
    minutes: Vec<u8>, hours: Vec<u8>, doms: Vec<u8>, months: Vec<u8>, dows: Vec<u8>,
}
impl CronMatcher {
    pub fn matches(&self, dt: &DateTime<Local>) -> bool { /* all 5 fields must match */ }
}
```

`parse_cron_list(s)` splits on `;` and returns `Vec<CronMatcher>`. A poll is in-window when the current time matches **any** clause.

Supported syntax per field: `*`, `n`, `n-m`, `*/n`, `n-m/n`, comma-separated combinations.

## Birthday and Jour J features

Both features are computed in `departure/model.rs`:

- `DepartureBoard::load_birthdays(file)` — reads `data/birthdays.json`, filters entries matching today's `DD/MM`, returns `Vec<String>` of display names (with age appended when birth year is present in the `DD/MM/YYYY` format).
- `DepartureBoard::compute_jour_j(date_str)` — parses a `DD/MM/YYYY` string, returns `Some(days_remaining)` when the date is today or in the future.

The board JSON includes these as optional fields:
```json
{
  "birthdays_today": ["Jean Martin (41)", "Claire Dupont"],
  "jour_j": [74, "♥ Mariage Coline et Hugo"]
}
```

## Concurrency model

| Primitive | Used for |
|-----------|----------|
| `tokio::spawn` | CTS poll task, weather poll task, per-WS send/recv task pair |
| `broadcast::channel` | Fan-out departure JSON (with weather) to N WebSocket clients |
| `RwLock<String>` | `monitoring_ref`, `jour_j_date`, `jour_j_label`, `latest` snapshot |
| `RwLock<Option<WeatherSnapshot>>` | Latest cached weather data |
| `RwLock<Option<WeatherCoords>>` | Resolved Meteoblue coordinates |
| `RwLock<Option<Vec<u8>>>` | Latest Pixoo64 frame PNG (simulation preview) |
| `AtomicI64` | `cts_next_poll_at` timestamp — lock-free reads from status endpoint |
| `Notify` | `poll_trigger` — zero-overhead wakeup from config changes |
| `tokio::select!` | Time-based or event-based poll wakeup; WS send/recv race |

## Key design decisions

**Single binary with embedded assets.** `rust-embed` compiles the `static/` folder into the binary at build time, so deployment is a single file copy.

**Renderer abstraction (`DisplayRenderer` trait).** The polling task writes to a `Vec<Box<dyn DisplayRenderer>>` and knows nothing about HTTP or WebSockets. The web renderer and the Pixoo64 renderer are both impls of this two-method trait, making them independently optional.

**API-agnostic domain model.** `DepartureBoard` (in `departure/model.rs`) is the boundary between CTS-specific SIRI structs and every consumer (web renderer, Pixoo64 renderer, simulation, offline boards). This keeps the API layer swappable.

**Weather injected at broadcast time.** Rather than a separate WebSocket message for weather, the weather snapshot is merged into the `DepartureBoard` JSON before each broadcast. Clients need no special handling — `board.weather` is just an optional field on the same message they already process.

**Weather re-broadcasts cached board.** When a new weather snapshot arrives, `store_and_rebroadcast` patches `state.latest` and sends it again. This solves the startup race where weather arrives after the first CTS broadcast.

**Independent simulation flags.** `cts_simulation` and `meteoblue_simulation` are independent. Either, both, or neither can be enabled, making partial offline testing straightforward.

**City-name location.** The Meteoblue location is configured as a human-readable city name. Coordinates are resolved once at startup via the Meteoblue `query3` search API and cached in `AppState::meteoblue_coords`. This avoids asking the user to supply lat/lon manually.

**Crontab gating without dependencies.** The 5-field crontab parser is implemented from scratch in `web/mod.rs` (~80 lines). No external crate is needed; `CronMatcher` is the only abstraction.

**Birthday JSON is static; age is computed at runtime.** The birthdays file stores only names and dates (no ages). Ages are computed dynamically from the birth year on each board update, so the file never goes stale.

## Configuration reference

### CTS keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `cts_api_token` | string | — | Inline CTS API token |
| `cts_api_token_file` | path | — | Path to file containing the token |
| `cts_monitoring_ref` | string | — | Logical stop code (e.g. `"298B"`) |
| `cts_polling_interval_minutes` | u64 | — | API query frequency |
| `cts_max_stop_visits` | u32 | `10` | Max departures per API call |
| `cts_vehicle_mode` | string? | — | Filter: `"tram"`, `"bus"`, `"coach"` |
| `cts_simulation` | bool | `false` | Use fake CTS data; never contact CTS API |
| `cts_always_query` | bool | `true` | Ignore time windows; poll 24/7 |
| `cts_query_intervals` | string? | — | 5-field crontab expressions (`;`-separated) |
| `cts_demo_lines` | u8? | `4` | Lines to show in simulation mode (1–4) |

### Server key

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `listen_addr` | string | `"0.0.0.0:3000"` | Web server bind address |

### Meteoblue keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `meteoblue_enabled` | bool | `false` | Enable the weather widget |
| `meteoblue_api_key` | string | — | Inline Meteoblue API key |
| `meteoblue_api_key_file` | path | — | Path to file containing the key |
| `meteoblue_location` | string | — | City name, resolved to coordinates at startup |
| `meteoblue_polling_interval_minutes` | u64 | `60` | Weather refresh frequency |
| `meteoblue_simulation` | bool | `false` | Use fixed offline weather |
| `meteoblue_always_query` | bool | `true` | Ignore time windows; poll 24/7 |
| `meteoblue_query_intervals` | string? | — | 5-field crontab expressions (`;`-separated) |

### Pixoo64 keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `pixoo64_enabled` | bool | `false` | Enable the Pixoo64 display renderer |
| `pixoo64_address` | string? | — | Device IP address (e.g. `"192.168.1.42"`) |
| `pixoo64_simulation` | bool | `false` | Render PNG preview only; no device calls |
| `pixoo64_refresh_interval_seconds` | u64? | `1` | Display refresh rate |

### Birthday keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `birthday_enabled` | bool | `false` | Show today's birthdays on the board |
| `birthday_file` | path? | `"data/birthdays.json"` | Path to the birthday JSON file |

### Jour J keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `jour_j_enabled` | bool | `false` | Show the Jour J countdown on the board |
| `jour_j_date` | string? | — | Target date in `DD/MM/YYYY` format |
| `jour_j_label` | string? | — | Event label (e.g. `"Noël"`) |

## REST API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/ws` | WebSocket upgrade; streams `DepartureBoard` JSON |
| `GET` | `/api/stops` | List all logical stops (deduplicated, sorted by name) |
| `GET` | `/api/stops/:code/details` | Physical stops and line/direction pairs under a logical code |
| `POST` | `/api/config` | `{"monitoring_ref":"…"}` — change monitored stop at runtime |
| `POST` | `/api/jour-j` | `{"date":"DD/MM/YYYY","label":"…"}` — update Jour J at runtime |
| `GET` | `/api/status` | CTS polling state + Meteoblue weather status + Jour J config |
| `GET` | `/api/pixoo64/preview` | Latest Pixoo64 frame as PNG (simulation mode only) |
| `GET` | `/*` | Embedded static files (`index.html`, `app.js`, `style.css`) |

## Dependencies

| Crate | Role |
|-------|------|
| `tokio` | Async runtime (multi-thread, signals, timers, sync) |
| `axum` | Web framework with WebSocket support |
| `tower-http` | Gzip compression middleware |
| `reqwest` | HTTP client (rustls TLS, JSON) |
| `serde` / `serde_json` | Serialization |
| `toml` | Config file parsing |
| `chrono` | DateTime handling with timezone support |
| `rust-embed` | Compile-time static asset embedding |
| `mime_guess` | Content-type detection for static files |
| `tracing` / `tracing-subscriber` | Structured logging |
| `url` | URL building for API queries |
| `anyhow` | Ergonomic error propagation |
| `futures-util` | WebSocket sink/stream combinators |
| `image` / `png` | Pixoo64 frame encoding |
