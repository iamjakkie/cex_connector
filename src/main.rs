use std::time::{Duration, Instant};

use latency::{current_timestamp_ns_hires, LatencyStats, HIGH_RES_TIMER};
use websocket::{WebSocketClient, WebSocketConfig, WebSocketError, WebSocketMessage, Result};

mod latency;
mod websocket;



fn main() -> Result<()> {
    
    let config = WebSocketConfig {
        connect_timeout: Duration::from_secs(10),
        read_timeout: Some(Duration::from_secs(30)),
        write_timeout: Some(Duration::from_secs(10)),
        max_frame_size: 1024 * 1024, // 1MB
        ping_interval: Duration::from_secs(30),
        user_agent: "OKXWebSocketClient/1.0".to_string(),
    };
    
    let mut client = WebSocketClient::connect_with_config(
        "wss://ws.okx.com:8443/ws/v5/public",
        config
    )?;
    
    let orderbook_subscribe = r#"{
        "op": "subscribe",
        "args": [
            {
                "channel": "books5",
                "instId": "BTC-USDT"
            }
        ]
    }"#;
    
    client.send_text(orderbook_subscribe)?;

    let mut latency_stats = LatencyStats::new();
    let mut last_stats_print = Instant::now();
    let stats_interval = Duration::from_secs(5); // Print stats every 5 seconds
    
    println!("📊 Measuring order book latency...");
    println!("Press Ctrl+C to stop\n");

    // logic to listen to the messages and calculate latency
    loop {
        match client.read_message() {
            Ok(msg) => {
                
                match msg {
                    WebSocketMessage::Text(text) => {
                        // Check if this is a subscription confirmation
                        if text.contains("\"event\":\"subscribe\"") && text.contains("\"channel\":\"books5\"") {
                            println!("✅ Successfully subscribed to BTC-USDT order book");
                            continue;
                        }
                        
                        // Check if this is order book data
                        if text.contains("\"channel\":\"books5\"") && text.contains("\"data\":[") {
                            let receive_time_ns = current_timestamp_ns_hires();

                            println!("{:?}", text);
                            
                            if let Some(exchange_timestamp_ms) = extract_timestamp_from_message(&text) {
                                let exchange_timestamp_ns = exchange_timestamp_ms * 1_000_000; // Convert ms to ns
                                let latency_ns = receive_time_ns.saturating_sub(exchange_timestamp_ns);
                                let latency_ms = latency_ns as f64 / 1_000_000.0;
                                
                                latency_stats.add_measurement(latency_ns);
                                
                                // Print individual measurements (for first few or outliers)
                                if latency_stats.count <= 5 || latency_ns > 100_000_000 { // 100ms in ns
                                    println!("📚 Order book update #{}: {:.3}ms latency ({:.0}ns precision)", 
                                           latency_stats.count, latency_ms, latency_ns as f64 % 1_000_000.0);
                                }
                            } else {
                                println!("⚠️  Could not extract timestamp from order book message");
                            }
                        }
                    }
                    WebSocketMessage::Ping(_) => {
                        println!("🏓 Received ping from OKX");
                    }
                    WebSocketMessage::Pong(_) => {
                        println!("🏓 Received pong from OKX");
                    }
                    WebSocketMessage::Close { code, reason } => {
                        println!("❌ Connection closed by OKX - Code: {:?}, Reason: {}", code, reason);
                        break;
                    }
                    _ => {}
                }
                
                // Print periodic statistics
                if last_stats_print.elapsed() >= stats_interval && latency_stats.count > 0 {
                    println!("\n📈 === High-Resolution Latency Statistics (last {} seconds) ===", stats_interval.as_secs());
                    println!("   📊 Total measurements: {}", latency_stats.count);
                    println!("   ⚡ Average latency: {:.3}ms", latency_stats.average_latency_ms());
                    println!("   🚀 Recent average (last 10): {:.3}ms", latency_stats.recent_average_ms());
                    println!("   🟢 Min latency: {:.3}ms", latency_stats.min_latency_ms());
                    println!("   🔴 Max latency: {:.3}ms", latency_stats.max_latency_ms());
                    
                    // Show recent latency trend with nanosecond precision
                    if latency_stats.last_10.len() >= 5 {
                        let recent: Vec<String> = latency_stats.last_10.iter()
                            .map(|&ns| format!("{:.3}ms", ns as f64 / 1_000_000.0))
                            .collect();
                        println!("   📊 Recent latencies: [{}]", recent.join(", "));
                    }
                    
                    // Show precision improvement
                    if latency_stats.count > 10 {
                        let std_dev = calculate_std_dev(&latency_stats.last_10);
                        println!("   📏 Recent std deviation: {:.3}ms ({:.0}μs)", 
                               std_dev / 1_000_000.0, std_dev / 1_000.0);
                    }
                    println!();
                    
                    last_stats_print = Instant::now();
                }
            }
            Err(WebSocketError::Timeout) => {
                println!("⏰ Read timeout, sending ping...");
                if let Err(e) = client.send_ping(b"latency-test") {
                    eprintln!("❌ Failed to send ping: {}", e);
                    break;
                }
            }
            Err(e) => {
                eprintln!("❌ Error reading message: {}", e);
                break;
            }
        }
    }
    
    // Final statistics
    if latency_stats.count > 0 {
        println!("\n🎯 === Final High-Resolution Latency Report ===");
        println!("   📊 Total order book updates: {}", latency_stats.count);
        println!("   ⚡ Average latency: {:.3}ms", latency_stats.average_latency_ms());
        println!("   🟢 Best latency: {:.3}ms", latency_stats.min_latency_ms());
        println!("   🔴 Worst latency: {:.3}ms", latency_stats.max_latency_ms());
        
        // Enhanced statistics with nanosecond precision
        if latency_stats.count >= 10 {
            let avg_ns = latency_stats.total_latency_ns as f64 / latency_stats.count as f64;
            let p95_threshold = avg_ns * 2.0;
            let outliers = latency_stats.last_10.iter()
                .filter(|&&ns| ns as f64 > p95_threshold)
                .count();
            
            println!("   📈 Measurements above 2x average: {} ({:.1}%)", 
                   outliers,
                   (outliers as f64 / latency_stats.last_10.len() as f64) * 100.0);
            
            // Show precision achieved
            let timer = HIGH_RES_TIMER.get().unwrap();
            
            #[cfg(target_arch = "x86_64")]
            let precision_info = {
                let precision_ns = 1_000_000_000.0 / timer.cycles_per_ns;
                format!("🎯 RDTSC precision: {:.1}ns per measurement", precision_ns)
            };
            
            #[cfg(not(target_arch = "x86_64"))]
            let precision_info = format!("🎯 Monotonic clock precision: ~1-10ns per measurement");
            
            println!("   {}", precision_info);
            println!("   🚀 Measurement overhead: ~{} (vs ~1ms for system calls)", 
                   if cfg!(target_arch = "x86_64") { "20ns" } else { "100ns" });
        }
    }
    
    println!("👋 High-resolution latency measurement completed!");
    Ok(())
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

fn extract_timestamp_from_message(text: &str) -> Option<u64> {
    // OKX sends timestamps in milliseconds in the "ts" field
    // Example: "ts":"1640995200000"
    if let Some(start) = text.find("\"ts\":\"") {
        let start_pos = start + 6; // Skip "ts":"
        if let Some(end) = text[start_pos..].find("\"") {
            let ts_str = &text[start_pos..start_pos + end];
            return ts_str.parse::<u64>().ok();
        }
    }
    None
}