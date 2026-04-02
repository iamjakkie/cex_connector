# cex_connector

A Rust tool for measuring WebSocket latency to cryptocurrency exchanges. Connects to OKX's public order book feed and computes end-to-end latency using a high-resolution timer.

## What it does

- Subscribes to the OKX `books5` channel (BTC-USDT top-5 order book)
- Extracts the exchange timestamp (`ts` field) from each update
- Compares it to the local receive time using a calibrated high-resolution timer
- Prints per-update latency and rolling statistics every 5 seconds

## High-resolution timing

On **x86_64**: uses the `RDTSC` instruction, calibrated against `SystemTime` at startup to convert cycles to nanoseconds. Overhead is ~20ns per measurement.

On **aarch64 / other**: falls back to `Instant` (monotonic clock), ~1–10ns overhead.

## Architecture

| File | Description |
|---|---|
| `src/main.rs` | Entry point — connects to OKX, runs the measurement loop |
| `src/latency.rs` | `HighResTimer`, `LatencyStats`, timestamp helpers |
| `src/websocket.rs` | Custom WebSocket client (TLS via rustls, full RFC 6455 framing) |

> **Note**: `main.rs` currently uses `tungstenite` directly. The custom `WebSocketClient` in `websocket.rs` is an alternative implementation kept for comparison.

## Sample output

```
Measuring order book latency...
Press Ctrl+C to stop

Successfully subscribed to BTC-USDT order book
Order book update #1: 166.572ms latency
Order book update #2: 161.438ms latency

=== High-Resolution Latency Statistics (last 5 seconds) ===
   Total measurements: 20
   Average latency:    166.111ms
   Recent avg (10):    169.565ms
   Min latency:        159.323ms
   Max latency:        233.551ms
   Recent latencies:   [161.6ms, 160.4ms, 167.6ms, 159.5ms, 164.0ms, ...]
   Recent std dev:     21.472ms
```

## Build and run

```bash
cargo build --release
cargo run --release
```

Press `Ctrl+C` to stop. A final summary is printed on exit.

## Dependencies

- [`tungstenite`](https://crates.io/crates/tungstenite) — WebSocket (current active connection)
- [`rustls`](https://crates.io/crates/rustls) + [`webpki-roots`](https://crates.io/crates/webpki-roots) — TLS for the custom client
- [`sha1`](https://crates.io/crates/sha1) + [`base64`](https://crates.io/crates/base64) — WebSocket handshake in the custom client
- [`tokio`](https://crates.io/crates/tokio) — async runtime (available, not yet used in the hot path)
- [`anyhow`](https://crates.io/crates/anyhow) — error handling
