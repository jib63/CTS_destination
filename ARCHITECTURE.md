# Architecture — CTS Departures

A single-binary Rust web application that polls the CTS (Compagnie des Transports Strasbourgeois) SIRI 2.0 API and serves a live departure board over WebSocket to connected browsers.

## High-level overview

```
┌─────────────────────────────────────────────────────────┐
│                        main.rs                          │
│  load config → build AppState → spawn poll task         │
│              → start Axum web server                    │
└────────────────────────┬────────────────────────────────┘
                         │  Arc<AppState>
           ┌─────────────┴─────────────┐
           ▼                           ▼
┌──────────────────┐        ┌─────────────────────┐
│  Polling task    │        │    Axum web server  │
│  api/client.rs   │        │    server/router.rs │
│                  │        │                     │
│  fetch_departures│        │  GET  /ws           │
│  simulate_board  │        │  GET  /api/stops    │
│  offline_board   │        │  GET  /api/stops/   │
│        │         │        │       :code/details │
│        ▼         │        │  POST /api/config   │
│  WebRenderer     │        │  GET  /api/status   │
│  (broadcast tx)──┼────────┼──► WS clients       │
└──────────────────┘        └─────────────────────┘
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
└── server/
    ├── mod.rs
    ├── router.rs         Axum router, REST handlers, embedded static files
    └── ws.rs             WebSocket connection handling
```

## Data flow

### Startup

1. `main.rs` reads `config.toml` (or a path from `argv[1]`).
2. `AppState` is constructed: HTTP client, broadcast channel, `RwLock`s for mutable config.
3. A Tokio task is spawned for the poll loop.
4. The Axum web server is started; both share the same `Arc<AppState>`.

### Poll loop (`api/client::poll_loop`)

```
loop:
  sleep until next_poll  (or wake early via poll_trigger Notify)
  read monitoring_ref    (RwLock)

  if outside query_intervals window:
      push offline DepartureBoard → renderers
      sleep until window opens

  elif simulation mode:
      api/simulation::simulate_board()  → renderers

  else:
      api/client::fetch_departures()    → renderers
      clamp interval to API's ShortestPossibleCycle

  schedule next_poll, store unix timestamp in AtomicI64
```

### WebSocket connection (`server/ws`)

```
client connects
  → subscribe to broadcast channel   (avoids startup race)
  → send cached `latest` snapshot    (immediate paint)
  → relay every broadcast message to client
  → detect close / lagged receiver
```

### Config hot-reload (`POST /api/config`)

```
validate new monitoring_ref
→ save_monitoring_ref()   rewrites config.toml line in-place (preserves comments)
→ state.monitoring_ref.write()   updates RwLock
→ state.poll_trigger.notify_one()  wakes poll loop immediately
```

## Concurrency model

| Primitive | Used for |
|-----------|----------|
| `tokio::spawn` | Polling task, per-WS send/recv task pair |
| `broadcast::channel` | Fan-out departure JSON to N WebSocket clients |
| `RwLock<String>` | `monitoring_ref` and `latest` snapshot (infrequently written) |
| `AtomicI64` | `next_poll_at` timestamp — lock-free reads from status endpoint |
| `Notify` | `poll_trigger` — zero-overhead wakeup from config changes |
| `tokio::select!` | Time-based or event-based poll wakeup; WS send/recv race |

## Key design decisions

**Single binary with embedded assets.** `rust-embed` compiles the `static/` folder into the binary at build time, so deployment is a single file copy.

**Renderer abstraction (`DisplayRenderer` trait).** The polling task writes to a `Vec<Box<dyn DisplayRenderer>>` and knows nothing about HTTP or WebSockets. Adding a new output (e.g. a Pixoo64 LED panel) requires only a new impl of this two-method trait.

**API-agnostic domain model.** `DepartureBoard` (in `departure/model.rs`) is the boundary between CTS-specific SIRI structs and every consumer (web renderer, simulation, offline boards). This keeps the API layer swappable.

**Time-window gating.** Polling can be restricted to service hours via `query_intervals`. The `offline_msg_and_sleep` helper computes both the human-readable gap message and the exact sleep duration, handling midnight wraparound.

**Simulation mode.** `api/simulation::simulate_board` produces live-looking data with per-poll jitter. It uses the same `DisplayRenderer` path as live mode, so the UI is indistinguishable during development.

## Configuration reference

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `api_token` | string | — | Inline CTS API token |
| `api_token_file` | path | — | Path to file containing the token |
| `monitoring_ref` | string | — | Logical stop code to monitor (e.g. `"298A"`) |
| `polling_interval_minutes` | u64 | — | API query frequency |
| `max_stop_visits` | u32 | `10` | Max departures per API call |
| `vehicle_mode` | string? | — | Filter: `"tram"`, `"bus"`, `"coach"` |
| `listen_addr` | string | `"0.0.0.0:3000"` | Web server bind address |
| `simulation` | bool | `false` | Use fake data; never contact API |
| `always_query` | bool | `true` | Ignore time windows; poll 24/7 |
| `query_intervals` | string? | — | Active windows, e.g. `"6:02-9:58;14:03-17:59"` |

## REST API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/ws` | WebSocket upgrade; streams `DepartureBoard` JSON |
| `GET` | `/api/stops` | List all logical stops (deduplicated, sorted by name) |
| `GET` | `/api/stops/:code/details` | Physical stops and line/direction pairs under a logical code |
| `POST` | `/api/config` | `{"monitoring_ref":"…"}` — change monitored stop at runtime |
| `GET` | `/api/status` | Polling state, time window, next poll timestamp |
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
