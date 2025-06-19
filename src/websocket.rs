use std::fmt;
use std::io::{Read, Write, BufRead, BufReader};
use std::net::{TcpStream, ToSocketAddrs};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::Result;
use base64::prelude::*;
use std::error::Error as StdError;


const OPCODE_CONTINUATION: u8 = 0x0;
const OPCODE_TEXT: u8 = 0x1;
const OPCODE_BINARY: u8 = 0x2;
const OPCODE_CLOSE: u8 = 0x8;
const OPCODE_PING: u8 = 0x9;
const OPCODE_PONG: u8 = 0xA;

const CLOSE_NORMAL: u16 = 1000;
const CLOSE_GOING_AWAY: u16 = 1001;
const CLOSE_PROTOCOL_ERROR: u16 = 1002;
const CLOSE_UNSUPPORTED: u16 = 1003;
const CLOSE_INVALID_DATA: u16 = 1007;
const CLOSE_POLICY_VIOLATION: u16 = 1008;
const CLOSE_MESSAGE_TOO_BIG: u16 = 1009;

const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024; // 16MB max frame size
const WEBSOCKET_MAGIC_STRING: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const PING_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum WebSocketError {
    Io(std::io::Error),
    InvalidUtf8(std::string::FromUtf8Error),
    ProtocolError(String),
    HandshakeError(String),
    ConnectionClosed,
    FrameTooLarge,
    InvalidCloseCode(u16),
    Timeout,
}

impl fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebSocketError::Io(e) => write!(f, "IO error: {}", e),
            WebSocketError::InvalidUtf8(e) => write!(f, "Invalid UTF-8: {}", e),
            WebSocketError::ProtocolError(s) => write!(f, "Protocol error: {}", s),
            WebSocketError::HandshakeError(s) => write!(f, "Handshake error: {}", s),
            WebSocketError::ConnectionClosed => write!(f, "Connection closed"),
            WebSocketError::FrameTooLarge => write!(f, "Frame too large"),
            WebSocketError::InvalidCloseCode(code) => write!(f, "Invalid close code: {}", code),
            WebSocketError::Timeout => write!(f, "Operation timed out"),
        }
    }
}

impl StdError for WebSocketError {}

impl From<std::io::Error> for WebSocketError {
    fn from(err: std::io::Error) -> Self {
        WebSocketError::Io(err)
    }
}

impl From<std::string::FromUtf8Error> for WebSocketError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        WebSocketError::InvalidUtf8(err)
    }
}

// type Result<T> = std::result::Result<T, WebSocketError>;

#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub max_frame_size: usize,
    pub ping_interval: Duration,
    pub user_agent: String,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_TIMEOUT,
            read_timeout: DEFAULT_TIMEOUT,
            write_timeout: DEFAULT_TIMEOUT,
            max_frame_size: MAX_FRAME_SIZE,
            ping_interval: PING_INTERVAL,
            user_agent: "WebsocketClient/1.0".to_string(),
        }
    }
}

struct WebSocketClient {
    stream: TcpStream,
    config: WebSocketConfig,
    last_ping: Instant,
    closed: bool,
}

impl WebSocketClient {
    pub fn connect<A: ToSocketAddrs>(addr: A, host: &str, path: &str) -> Result<Self> {
        Self::connect_with_config(addr, host, path, WebSocketConfig::default())
    }

    fn connect_with_config<A: ToSocketAddrs>(
        addr: A,
        host: &str,
        path: &str,
        config: WebSocketConfig,
    ) -> Result<Self> {
        let stream = TcpStream::connect_timeout(
            &addr.to_socket_addrs()?.next().ok_or_else(|| {
                WebSocketError::ProtocolError("No valid address found".to_string())
            })?,
            config.connect_timeout
        )?;

        stream.set_read_timeout(config.read_timeout)?;
        stream.set_write_timeout(config.write_timeout)?;
        stream.set_nodelay(true)?;

        let mut client = WebSocketClient {
            stream,
            config,
            last_ping: Instant::now(),
            closed: false,
        };

        client.perform_handshake(host, path)?;
        Ok(client)
    }
}


        // let mut stream = TcpStream::connect(format!("{}:{}", host, port))?;

        // let key = BASE64_STANDARD.encode(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);

        // let request = format!(
        //     "GET {} HTTP/1.1\r\n\
        //      Host: {}:{}\r\n\
        //      Upgrade: websocket\r\n\
        //      Connection: Upgrade\r\n\
        //      Sec-WebSocket-Key: {}\r\n\
        //      Sec-WebSocket-Version: 13\r\n\
        //      \r\n",
        //     path, host, port, key
        // );

        // stream.write_all(request.as_bytes())?;

        // let mut reader = BufReader::new(&stream);
        // let mut response_line = String::new();
        // reader.read_line(&mut response_line)?;

        // if !response_line.contains("101") {
        //     return Err(anyhow::anyhow!("Websocket handshake failed"));
        // }

        // let mut headers = HashMap::new();
        // loop {
        //     let mut line = String::new();
        //     reader.read_line(&mut line)?;
        //     if line.trim().is_empty() {
        //         break;
        //     }

        //     if let Some((key, value)) = line.split_once(": ") {
        //         headers.insert(key.to_lowercase(), value.trim().to_string());
        //     }
        // }

        // if headers.get("upgrade") != Some(&"websocket".to_string()) {
        //     return Err(anyhow::anyhow!("Upgrade header not present or incorrect"));
        // }

        // println!("Websocket connection established to {}:{}", host, port);

        // Ok( WebsocketClient{ stream })
//     }

//     fn send_text(&mut self, message: &str) -> Result<()> {
//         self.send_frame(OPCODE_TEXT, message.as_bytes())
//     }

//     fn send_binary(&mut self, data: &[u8]) -> Result<()> {
//         self.send_frame(OPCODE_BINARY, data)
//     }

//     fn send_ping(&mut self, data: &[u8]) -> Result<()> {
//         self.send_frame(OPCODE_PING, data)
//     }

//     fn send_pong(&mut self, data: &[u8]) -> Result<()> {
//         self.send_frame(OPCODE_PONG, data)
//     }

//     fn send_close(&mut self) -> Result<()> {
//         self.send_frame(OPCODE_CLOSE, &[])
//     }

//     fn send_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<()> {
//         let mut frame = Vec::new();

//         frame.push(0x80 | opcode); // FIN bit set and opcode

//         let payload_length = payload.len();
//         let mask_key = [0x12, 0x34, 0x56, 0x78]; // Example mask key, not used in this case

//         if payload_length < 126 {
//             frame.push(0x80 | payload_length as u8); // Mask bit set and payload length
//         } else if payload_length < 65536 {
//             frame.push(0x80 | 126);
//             frame.extend_from_slice(&(payload_length as u16).to_be_bytes());
//         } else {
//             frame.push(0x80 | 127);
//             frame.extend_from_slice(&(payload_length as u64).to_be_bytes());
//         }

//         frame.extend_from_slice(&mask_key);

//         for (i, &byte) in payload.iter().enumerate() {
//             frame.push(byte ^ mask_key[i % 4]); // Apply mask
//         }

//         self.stream.write_all(&frame)?;

//         Ok(())
//     }
// }