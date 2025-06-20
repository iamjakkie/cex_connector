use std::time::Duration;

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

    // logic to listen to the messages and calculate latency
    
    Ok(())
}