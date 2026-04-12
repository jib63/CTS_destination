# Architecture — CTS Departures

A single-binary Rust web application that polls the CTS (Compagnie des Transports Strasbourgeois) SIRI 2.0 API and serves a live departure board over WebSocket to connected browsers. An optional Meteoblue weather widget is displayed in the board footer.

## High-level overview

```
┌─────────────────────────────────────────────────────────────┐
│                          main.rs                            │
│  load config → build AppState → spawn CTS poll task         │
│                             → spawn weather poll task        │
│                             → start Axum web server          │
└────────────────────────┬────────────────────────────────────┘
                         │  Arc<AppState>
           ┌─────────────┼─────────────┐
           ▼             ▼             ▼
┌──────────────────┐  ┌────────────┐  ┌─────────────────────┐
│  CTS poll task   │  │ Weather    │  │   Axum web server   │
│  api/client.rs   │  │ poll task  │  │   server/router.rs  │
│                  │  │ weather/   │  │                     │
│  fetch_departs   │  │ client.rs  │  │  GET  /ws           │
│  simulate_board  │  │            │  │  GET  /api/stops    │
│  offline_board   │  │  fetch     │  │  GET  /api/stops/   │
│        │         │  │  weather   │  │       :code/details │
│        ▼         │  │  coords    │  │  POST /api/config   │
│  WebRenderer     │  │     │      │  │  GET  /api/status   │
│  (broadcast tx)──┼──┼─────┘      │  │                     │
│  patches weather │  │  patches   │  │                     │
│  into board JSON │◄─┘  latest +  │  │                     │
│  before broadcast│     rebroadcst │  │  ► WS clients       │
└──────────────────┘  └────────────┘  └─────────────────────┘
```

## Module map

```
src/
├── main.rs               Entry point, lifecycle, graceful shutdown
├── config.rs             TOML config loading and in-place persistence
│
├── api/
│   ├── mod.rs
│   ├── client.rs         CTS API client, poll loop, stop discovery
│   ├── model.rs          SIRI JSON data structures + ISO duration helper
│   └── simulation.rs     Offline fake-data generator
│
├── departure/
│   ├── mod.rs
│   └── model.rs          API-agnostic DepartureBoard domain model
│
├── display/
│   ├── mod.rs            DisplayRenderer trait
│   └── web.rs            AppState, WebRenderer, interval parsing
│
├── server/
│   ├── mod.rs
│   ├── router.rs         Axum router, REST handlers, embedded static files
│   └── ws.rs             WebSocket connection handling
│
└── weather/
    ├── mod.rs            Re-exports client, model, simulation
    ├── client.rs         Location resolution (Meteoblue query3 API) + poll loop
    ├── model.rs          MeteoblueResponse, WeatherCoords, WeatherSnapshot
    └── simulation.rs     Fixed offline WeatherSnapshot (Strasbourg values)
```

## Data flow

### Startup

1. `main.rs` reads `config.toml` (or a path from `argv[1]`).
2. `AppState` is constructed: HTTP client, broadcast channel, `RwLock`s for mutable config.
3. A Tokio task is spawned for the CTS poll loop.
4. If `meteoblue_enabled`, a second Tokio task is spawned for the weather poll loop. It resolves the city name to coordinates once at startup, then polls for weather every `meteoblue_polling_interval_minutes` minutes.
5. The Axum web server is started; all tasks share the same `Arc<AppState>`.

### CTS poll loop (`api/client::poll_loop`)

```
loop:
  sleep until next_poll  (or wake early via poll_trigger Notify)
  read monitoring_ref    (RwLock)

  if outside query_intervals window:
      push offline DepartureBoard → renderers
      sleep until window opens

  elif cts_simulation:
      api/simulation::simulate_board()  → renderers

  else:
      api/client::fetch_departures()    → renderers
      clamp interval to API's ShortestPossibleCycle

  if weather_enabled:
      board.weather = latest_weather.read()   (cached by weather task)

  schedule next_poll, store unix timestamp in AtomicI64
```

### Weather poll loop (`weather/client::weather_poll_loop`)

```
startup:
  resolve city name → (lat, lon, asl) via Meteoblue query3 API
  store in AppState::weather_coords

loop:
  if meteoblue_simulation:
      snap = simulation::simulate_weather()
  else:
      fetch Meteoblue basic-1h_basic-day API
      snap = WeatherSnapshot::from_response()

  store_and_rebroadcast(snap):
      latest_weather.write() = snap
      patch latest cached board JSON: board["weather"] = snap
      broadcast patched JSON to all WS clients

  sleep weather_polling_interval_minutes * 60 s
  (on error: log warning, retry after 5 minutes)
```

The `store_and_rebroadcast` step ensures the weather footer updates immediately even if the weather poll fires between two CTS polls.

### WebSocket connection (`server/ws`)

```
client connects
  → subscribe to broadcast channel   (avoids startup race)
  → send cached `latest` snapshot    (immediate paint, includes weather)
  → relay every broadcast message to client
  → detect close / lagged receiver
```

### Config hot-reload (`POST /api/config`)

```
validate new cts_monitoring_ref
→ save_monitoring_ref()   rewrites cts_monitoring_ref line in config.toml (preserves comments)
→ state.monitoring_ref.write()   updates RwLock
→ state.poll_trigger.notify_one()  wakes CTS poll loop immediately
```

## Concurrency model

| Primitive | Used for |
|-----------|----------|
| `tokio::spawn` | CTS poll task, weather poll task, per-WS send/recv task pair |
| `broadcast::channel` | Fan-out departure JSON (with weather) to N WebSocket clients |
| `RwLock<String>` | `monitoring_ref` and `latest` snapshot (infrequently written) |
| `RwLock<Option<WeatherSnapshot>>` | Latest cached weather data |
| `RwLock<Option<WeatherCoords>>` | Resolved Meteoblue coordinates |
| `AtomicI64` | `next_poll_at` timestamp — lock-free reads from status endpoint |
| `Notify` | `poll_trigger` — zero-overhead wakeup from config changes |
| `tokio::select!` | Time-based or event-based poll wakeup; WS send/recv race |

## Key design decisions

**Single binary with embedded assets.** `rust-embed` compiles the `static/` folder into the binary at build time, so deployment is a single file copy.

**Renderer abstraction (`DisplayRenderer` trait).** The polling task writes to a `Vec<Box<dyn DisplayRenderer>>` and knows nothing about HTTP or WebSockets. Adding a new output (e.g. a Pixoo64 LED panel) requires only a new impl of this two-method trait.

**API-agnostic domain model.** `DepartureBoard` (in `departure/model.rs`) is the boundary between CTS-specific SIRI structs and every consumer (web renderer, simulation, offline boards). This keeps the API layer swappable.

**Weather injected at broadcast time.** Rather than a separate WebSocket message for weather, the weather snapshot is merged into the `DepartureBoard` JSON before each broadcast. Clients need no special handling — `board.weather` is just an optional field on the same message they already process.

**Weather re-broadcasts cached board.** When a new weather snapshot arrives, `store_and_rebroadcast` patches `state.latest` (the cached board JSON) and sends it again. This solves the startup race where weather arrives after the first CTS broadcast.

**Independent simulation flags.** `cts_simulation` and `meteoblue_simulation` are independent. Either, both, or neither can be enabled, making partial offline testing straightforward.

**City-name location.** The Meteoblue location is configured as a human-readable city name. Coordinates are resolved once at startup via the Meteoblue `query3` search API and cached in `AppState::weather_coords`. This avoids asking the user to supply lat/lon manually.

**Time-window gating.** Polling can be restricted to service hours via `cts_query_intervals`. The `offline_msg_and_sleep` helper computes both the human-readable gap message and the exact sleep duration, handling midnight wraparound.

## Configuration reference

### CTS keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `cts_api_token` | string | — | Inline CTS API token |
| `cts_api_token_file` | path | — | Path to file containing the token |
| `cts_monitoring_ref` | string | — | Logical stop code to monitor (e.g. `"298A"`) |
| `cts_polling_interval_minutes` | u64 | — | API query frequency |
| `cts_max_stop_visits` | u32 | `10` | Max departures per API call |
| `cts_vehicle_mode` | string? | — | Filter: `"tram"`, `"bus"`, `"coach"` |
| `cts_simulation` | bool | `false` | Use fake CTS data; never contact CTS API |
| `cts_always_query` | bool | `true` | Ignore time windows; poll 24/7 |
| `cts_query_intervals` | string? | — | Active windows, e.g. `"6:02-9:58;14:03-17:59"` |

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
| `meteoblue_simulation` | bool | `false` | Use fixed offline weather; never contact Meteoblue |

## REST API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/ws` | WebSocket upgrade; streams `DepartureBoard` JSON (includes `weather` field when enabled) |
| `GET` | `/api/stops` | List all logical stops (deduplicated, sorted by name) |
| `GET` | `/api/stops/:code/details` | Physical stops and line/direction pairs under a logical code |
| `POST` | `/api/config` | `{"monitoring_ref":"…"}` — change monitored stop at runtime |
| `GET` | `/api/status` | CTS polling state + Meteoblue weather status (two-tab JSON) |
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
