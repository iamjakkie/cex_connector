use std::time::{Duration, Instant};

use latency::{current_timestamp_ns_hires, LatencyStats};
use serde::Deserialize;
use websocket::{WebSocketClient, WebSocketConfig, WebSocketMessage, Result};

mod latency;
mod websocket;

const OKX_WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/public";
const STATS_INTERVAL: Duration = Duration::from_secs(5);

// Minimal structs for OKX order book message deserialization
#[derive(Deserialize)]
struct OkxMessage {
    #[serde(default)]
    event: Option<String>,
    #[serde(default)]
    arg: Option<OkxArg>,
    #[serde(default)]
    data: Option<Vec<OkxBookData>>,
}

#[derive(Deserialize)]
struct OkxArg {
    channel: String,
}

#[derive(Deserialize)]
struct OkxBookData {
    ts: String,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WebSocketConfig {
        connect_timeout: Duration::from_secs(10),
        read_timeout: Some(Duration::from_secs(30)),
        write_timeout: Some(Duration::from_secs(10)),
        ping_interval: Duration::from_secs(30),
        ..Default::default()
    };

    let mut client = WebSocketClient::connect_with_config(OKX_WS_URL, config)?;

    let orderbook_subscribe = r#"{"op":"subscribe","args":[{"channel":"books5","instId":"BTC-USDT"}]}"#;
    client.send_text(orderbook_subscribe)?;

    let mut latency_stats = LatencyStats::default();
    let mut last_stats_print = Instant::now();

    println!("Measuring order book latency. Press Ctrl+C to stop.\n");

    loop {
        match client.read_message() {
            Ok(WebSocketMessage::Text(text)) => {
                let msg: OkxMessage = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!("Failed to parse message: {}", e);
                        continue;
                    }
                };

                // Subscription confirmation
                if msg.event.as_deref() == Some("subscribe") {
                    if let Some(arg) = &msg.arg {
                        tracing::info!("Subscribed to channel: {}", arg.channel);
                    }
                    continue;
                }

                // Order book data
                if let Some(data) = &msg.data {
                    let receive_time_ns = current_timestamp_ns_hires();

                    if let Some(entry) = data.first() {
                        match entry.ts.parse::<u64>() {
                            Ok(exchange_timestamp_ms) => {
                                let exchange_timestamp_ns = exchange_timestamp_ms * 1_000_000;
                                let latency_ns = receive_time_ns.saturating_sub(exchange_timestamp_ns);
                                let latency_ms = latency_ns as f64 / 1_000_000.0;

                                latency_stats.add_measurement(latency_ns);

                                if latency_stats.count <= 5 || latency_ns > 100_000_000 {
                                    println!(
                                        "Update #{}: {:.3}ms latency",
                                        latency_stats.count, latency_ms
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse timestamp '{}': {}", entry.ts, e);
                            }
                        }
                    }
                }
            }
            Ok(WebSocketMessage::Ping(_)) => {
                tracing::debug!("Received ping from server");
            }
            Ok(WebSocketMessage::Pong(_)) => {
                tracing::debug!("Received pong from server");
            }
            Ok(WebSocketMessage::Close { code, reason }) => {
                tracing::info!("Connection closed by server - code: {:?}, reason: {}", code, reason);
                break;
            }
            Ok(WebSocketMessage::Binary(_)) => {
                tracing::debug!("Received unexpected binary message");
            }
            Err(e) => {
                tracing::error!("Error reading message: {}", e);
                break;
            }
        }

        if last_stats_print.elapsed() >= STATS_INTERVAL && latency_stats.count > 0 {
            print_stats(&latency_stats, STATS_INTERVAL.as_secs());
            last_stats_print = Instant::now();
        }
    }

    if latency_stats.count > 0 {
        print_final_stats(&latency_stats);
    }

    println!("Done.");
    Ok(())
}

fn print_stats(stats: &LatencyStats, interval_secs: u64) {
    println!("\n=== Latency Statistics (last {}s) ===", interval_secs);
    println!("  Total measurements: {}", stats.count);
    println!("  Average:            {:.3}ms", stats.average_latency_ms());
    println!("  Recent avg (10):    {:.3}ms", stats.recent_average_ms());
    println!("  Min:                {:.3}ms", stats.min_latency_ms());
    println!("  Max:                {:.3}ms", stats.max_latency_ms());

    if stats.last_10.len() >= 5 {
        let recent: Vec<String> = stats.last_10.iter()
            .map(|&ns| format!("{:.3}ms", ns as f64 / 1_000_000.0))
            .collect();
        println!("  Recent:             [{}]", recent.join(", "));
    }

    if stats.count > 10 {
        let last_10: Vec<u64> = stats.last_10.iter().copied().collect();
        let std_dev = calculate_std_dev(&last_10);
        println!("  Std dev (recent):   {:.3}ms", std_dev / 1_000_000.0);
    }

    #[cfg(target_arch = "x86_64")]
    if let Some(timer) = latency::HIGH_RES_TIMER.get() {
        let precision_ns = 1_000_000_000.0 / timer.cycles_per_ns;
        println!("  RDTSC precision:    {:.1}ns", precision_ns);
    }

    println!();
}

fn print_final_stats(stats: &LatencyStats) {
    println!("\n=== Final Latency Report ===");
    println!("  Total updates: {}", stats.count);
    println!("  Average:       {:.3}ms", stats.average_latency_ms());
    println!("  Best:          {:.3}ms", stats.min_latency_ms());
    println!("  Worst:         {:.3}ms", stats.max_latency_ms());
}

fn calculate_std_dev(values: &[u64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }

    let mean = values.iter().sum::<u64>() as f64 / values.len() as f64;
    let variance = values.iter()
        .map(|&x| {
            let diff = x as f64 - mean;
            diff * diff
        })
        .sum::<f64>() / values.len() as f64;

    variance.sqrt()
}
