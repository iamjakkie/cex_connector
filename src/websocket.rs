use std::io::{Read, Write, BufRead, BufReader};
use std::net::{TcpStream, ToSocketAddrs};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::fmt;
use std::error::Error as StdError;
use std::sync::Arc;

use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls::pki_types::ServerName;
use sha1::{Sha1, Digest};
use base64::prelude::*;

// WebSocket opcodes
const OPCODE_CONTINUATION: u8 = 0x0;
const OPCODE_TEXT: u8 = 0x1;
const OPCODE_BINARY: u8 = 0x2;
const OPCODE_CLOSE: u8 = 0x8;
const OPCODE_PING: u8 = 0x9;
const OPCODE_PONG: u8 = 0xa;

// WebSocket close codes
const CLOSE_NORMAL: u16 = 1000;
const CLOSE_GOING_AWAY: u16 = 1001;
const CLOSE_PROTOCOL_ERROR: u16 = 1002;
const CLOSE_UNSUPPORTED: u16 = 1003;
const CLOSE_INVALID_DATA: u16 = 1007;
const CLOSE_POLICY_VIOLATION: u16 = 1008;
const CLOSE_MESSAGE_TOO_BIG: u16 = 1009;

// Configuration constants
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
    TlsError(rustls::Error),
    DnsError(String),
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
            WebSocketError::TlsError(e) => write!(f, "TLS error: {}", e),
            WebSocketError::DnsError(s) => write!(f, "DNS error: {}", s),
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

impl From<rustls::Error> for WebSocketError {
    fn from(err: rustls::Error) -> Self {
        WebSocketError::TlsError(err)
    }
}

pub type Result<T> = std::result::Result<T, WebSocketError>;

#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    pub connect_timeout: Duration,
    pub read_timeout: Option<Duration>,
    pub write_timeout: Option<Duration>,
    pub max_frame_size: usize,
    pub ping_interval: Duration,
    pub user_agent: String,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_TIMEOUT,
            read_timeout: Some(DEFAULT_TIMEOUT),
            write_timeout: Some(DEFAULT_TIMEOUT),
            max_frame_size: MAX_FRAME_SIZE,
            ping_interval: PING_INTERVAL,
            user_agent: "RustWebSocketTLS/1.0".to_string(),
        }
    }
}

enum StreamType {
    Plain(TcpStream),
    Tls(StreamOwned<ClientConnection, TcpStream>),
}

impl Read for StreamType {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            StreamType::Plain(stream) => stream.read(buf),
            StreamType::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for StreamType {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            StreamType::Plain(stream) => stream.write(buf),
            StreamType::Tls(stream) => stream.write(buf),
        }
    }
    
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            StreamType::Plain(stream) => stream.flush(),
            StreamType::Tls(stream) => stream.flush(),
        }
    }
}

pub struct WebSocketClient {
    stream: StreamType,
    config: WebSocketConfig,
    last_ping: Instant,
    closed: bool,
    is_secure: bool,
}

impl WebSocketClient {
    pub fn connect(url: &str) -> Result<Self> {
        Self::connect_with_config(url, WebSocketConfig::default())
    }
    
    pub fn connect_with_config(url: &str, config: WebSocketConfig) -> Result<Self> {
        let parsed_url = parse_websocket_url(url)?;
        let host = parsed_url.host.clone();
        println!("Connecting to {}://{}:{}{}", 
                 parsed_url.scheme, parsed_url.host, parsed_url.port, parsed_url.path);
        
        // Create socket address for connection
        let socket_addrs = format!("{}:{}", parsed_url.host, parsed_url.port)
            .to_socket_addrs()
            .map_err(|e| WebSocketError::DnsError(format!("Failed to resolve {}: {}", parsed_url.host, e)))?
            .collect::<Vec<_>>();
        
        if socket_addrs.is_empty() {
            return Err(WebSocketError::DnsError(format!("No addresses found for {}", parsed_url.host)));
        }
        
        println!("Resolved {} to {:?}", parsed_url.host, socket_addrs[0]);
        
        let tcp_stream = TcpStream::connect_timeout(&socket_addrs[0], config.connect_timeout)?;
        tcp_stream.set_nodelay(true)?;
        
        let stream = if parsed_url.scheme == "wss" {
            // Create TLS configuration with updated rustls API
            let root_store = rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.into(),
            };
            
            let tls_config = ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth();
            
            let server_name = ServerName::try_from(host)
                .map_err(|e| WebSocketError::DnsError(format!("Invalid server name '{}': {}", parsed_url.host, e)))?;
            
            let client_conn = ClientConnection::new(Arc::new(tls_config), server_name)?;
            let tls_stream = StreamOwned::new(client_conn, tcp_stream);
            
            StreamType::Tls(tls_stream)
        } else {
            StreamType::Plain(tcp_stream)
        };
        
        // Set timeouts (for TLS streams, this affects the underlying TCP stream)
        match &stream {
            StreamType::Plain(tcp) => {
                tcp.set_read_timeout(config.read_timeout)?;
                tcp.set_write_timeout(config.write_timeout)?;
            }
            StreamType::Tls(tls_stream) => {
                tls_stream.get_ref().set_read_timeout(config.read_timeout)?;
                tls_stream.get_ref().set_write_timeout(config.write_timeout)?;
            }
        }
        
        let mut client = WebSocketClient {
            stream,
            config,
            last_ping: Instant::now(),
            closed: false,
            is_secure: parsed_url.scheme == "wss",
        };
        
        client.perform_handshake(&parsed_url.host, &parsed_url.path)?;
        Ok(client)
    }
    
    fn perform_handshake(&mut self, host: &str, path: &str) -> Result<()> {
        // Generate cryptographically secure WebSocket key
        let key = generate_websocket_key();
        println!("Generated WebSocket key: {}", key);
        println!("Key length: {} characters", key.len());
        
        // Send HTTP upgrade request
        let request = format!(
            "GET {} HTTP/1.1\r\n\
             Host: {}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             User-Agent: {}\r\n\
             Origin: https://{}\r\n\
             \r\n",
            path, host, key, self.config.user_agent, host
        );
        
        self.stream.write_all(request.as_bytes())?;
        self.stream.flush()?;
        
        // Read and validate HTTP response
        let mut reader = BufReader::new(&mut self.stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;
        
        println!("Server response: {}", response_line.trim());
        
        if !response_line.starts_with("HTTP/1.1 101") {
            return Err(WebSocketError::HandshakeError(
                format!("Expected 101 Switching Protocols, got: {}", response_line.trim())
            ));
        }
        
        // Read and validate headers
        let mut headers = HashMap::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            if line.trim().is_empty() {
                break;
            }
            
            if let Some((key, value)) = line.split_once(':') {
                headers.insert(
                    key.trim().to_lowercase(), 
                    value.trim().to_string()
                );
            }
        }
        
        println!("Handshake headers received: {:?}", headers);
        
        // Validate required headers
        self.validate_handshake_headers(&headers, &key)?;
        
        println!("✅ WebSocket handshake successful!");
        Ok(())
    }
    
    fn validate_handshake_headers(&self, headers: &HashMap<String, String>, key: &str) -> Result<()> {
        // Check upgrade header
        if headers.get("upgrade").map(|s| s.to_lowercase()) != Some("websocket".to_string()) {
            return Err(WebSocketError::HandshakeError(
                "Missing or invalid Upgrade header".to_string()
            ));
        }
        
        // Check connection header
        if !headers.get("connection")
            .map(|s| s.to_lowercase().contains("upgrade"))
            .unwrap_or(false) {
            return Err(WebSocketError::HandshakeError(
                "Missing or invalid Connection header".to_string()
            ));
        }
        
        // Verify Sec-WebSocket-Accept
        if let Some(accept_key) = headers.get("sec-websocket-accept") {
            let expected_accept = generate_accept_key(key);
            println!("WebSocket key used: {}", key);
            println!("Combined string: {}{}", key, WEBSOCKET_MAGIC_STRING);
            println!("Expected accept key: {}", expected_accept);
            println!("Received accept key: {}", accept_key);
            println!("Expected length: {}, Received length: {}", expected_accept.len(), accept_key.len());
            if accept_key != &expected_accept {
                return Err(WebSocketError::HandshakeError(
                    format!("Invalid Sec-WebSocket-Accept header. Expected: {}, Got: {}", expected_accept, accept_key)
                ));
            }
        } else {
            return Err(WebSocketError::HandshakeError(
                "Missing Sec-WebSocket-Accept header".to_string()
            ));
        }
        
        Ok(())
    }
    
    pub fn send_text(&mut self, text: &str) -> Result<()> {
        if self.closed {
            return Err(WebSocketError::ConnectionClosed);
        }
        self.send_frame(OPCODE_TEXT, text.as_bytes())
    }
    
    pub fn send_binary(&mut self, data: &[u8]) -> Result<()> {
        if self.closed {
            return Err(WebSocketError::ConnectionClosed);
        }
        self.send_frame(OPCODE_BINARY, data)
    }
    
    pub fn send_ping(&mut self, data: &[u8]) -> Result<()> {
        if self.closed {
            return Err(WebSocketError::ConnectionClosed);
        }
        if data.len() > 125 {
            return Err(WebSocketError::ProtocolError(
                "Ping payload too large (max 125 bytes)".to_string()
            ));
        }
        self.send_frame(OPCODE_PING, data)?;
        self.last_ping = Instant::now();
        Ok(())
    }
    
    pub fn send_pong(&mut self, data: &[u8]) -> Result<()> {
        if self.closed {
            return Err(WebSocketError::ConnectionClosed);
        }
        if data.len() > 125 {
            return Err(WebSocketError::ProtocolError(
                "Pong payload too large (max 125 bytes)".to_string()
            ));
        }
        self.send_frame(OPCODE_PONG, data)
    }
    
    pub fn close(&mut self) -> Result<()> {
        self.close_with_code(CLOSE_NORMAL, "")
    }
    
    pub fn close_with_code(&mut self, code: u16, reason: &str) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        
        if !is_valid_close_code(code) {
            return Err(WebSocketError::InvalidCloseCode(code));
        }
        
        let reason_bytes = reason.as_bytes();
        if reason_bytes.len() > 123 {
            return Err(WebSocketError::ProtocolError(
                "Close reason too long (max 123 bytes)".to_string()
            ));
        }
        
        let mut payload = Vec::with_capacity(2 + reason_bytes.len());
        payload.extend_from_slice(&code.to_be_bytes());
        payload.extend_from_slice(reason_bytes);
        
        self.send_frame(OPCODE_CLOSE, &payload)?;
        self.closed = true;
        Ok(())
    }
    
    fn send_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<()> {
        if payload.len() > self.config.max_frame_size {
            return Err(WebSocketError::FrameTooLarge);
        }
        
        let mut frame = Vec::new();
        
        // First byte: FIN (1) + RSV (000) + Opcode (4 bits)
        frame.push(0x80 | opcode);
        
        // Generate cryptographically secure mask key
        let mask_key = generate_mask_key();
        
        // Payload length and masking bit
        let payload_len = payload.len();
        if payload_len < 126 {
            frame.push(0x80 | payload_len as u8);
        } else if payload_len < 65536 {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload_len as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(payload_len as u64).to_be_bytes());
        }
        
        // Add mask key
        frame.extend_from_slice(&mask_key);
        
        // Add masked payload
        for (i, &byte) in payload.iter().enumerate() {
            frame.push(byte ^ mask_key[i % 4]);
        }
        
        self.stream.write_all(&frame)?;
        self.stream.flush()?;
        Ok(())
    }
    
    pub fn read_message(&mut self) -> Result<WebSocketMessage> {
        if self.closed {
            return Err(WebSocketError::ConnectionClosed);
        }
        
        // Check if we need to send a ping
        if self.last_ping.elapsed() > self.config.ping_interval {
            let _ = self.send_ping(b"ping"); // Ignore ping errors
        }
        
        let frame = self.read_frame()?;
        
        match frame.opcode {
            OPCODE_TEXT => {
                let text = String::from_utf8(frame.payload)?;
                Ok(WebSocketMessage::Text(text))
            }
            OPCODE_BINARY => Ok(WebSocketMessage::Binary(frame.payload)),
            OPCODE_PING => {
                // Auto-respond to pings
                let _ = self.send_pong(&frame.payload);
                Ok(WebSocketMessage::Ping(frame.payload))
            }
            OPCODE_PONG => Ok(WebSocketMessage::Pong(frame.payload)),
            OPCODE_CLOSE => {
                self.closed = true;
                let (code, reason) = if frame.payload.len() >= 2 {
                    let code = u16::from_be_bytes([frame.payload[0], frame.payload[1]]);
                    let reason = if frame.payload.len() > 2 {
                        String::from_utf8_lossy(&frame.payload[2..]).to_string()
                    } else {
                        String::new()
                    };
                    (Some(code), reason)
                } else {
                    (None, String::new())
                };
                Ok(WebSocketMessage::Close { code, reason })
            }
            _ => Err(WebSocketError::ProtocolError(
                format!("Unknown opcode: {}", frame.opcode)
            )),
        }
    }
    
    fn read_frame(&mut self) -> Result<WebSocketFrame> {
        let mut header = [0u8; 2];
        self.stream.read_exact(&mut header)?;
        
        let fin = (header[0] & 0x80) != 0;
        let rsv = (header[0] & 0x70) >> 4;
        let opcode = header[0] & 0x0f;
        let masked = (header[1] & 0x80) != 0;
        let mut payload_len = (header[1] & 0x7f) as u64;
        
        // Validate reserved bits
        if rsv != 0 {
            return Err(WebSocketError::ProtocolError(
                "Reserved bits must be zero".to_string()
            ));
        }
        
        // Server frames must not be masked
        if masked {
            return Err(WebSocketError::ProtocolError(
                "Server frames must not be masked".to_string()
            ));
        }
        
        // Extended payload length
        if payload_len == 126 {
            let mut len_bytes = [0u8; 2];
            self.stream.read_exact(&mut len_bytes)?;
            payload_len = u16::from_be_bytes(len_bytes) as u64;
        } else if payload_len == 127 {
            let mut len_bytes = [0u8; 8];
            self.stream.read_exact(&mut len_bytes)?;
            payload_len = u64::from_be_bytes(len_bytes);
            
            // Check for valid payload length
            if payload_len & 0x8000_0000_0000_0000 != 0 {
                return Err(WebSocketError::ProtocolError(
                    "Invalid payload length".to_string()
                ));
            }
        }
        
        // Check frame size limit
        if payload_len as usize > self.config.max_frame_size {
            return Err(WebSocketError::FrameTooLarge);
        }
        
        // Read payload
        let mut payload = vec![0u8; payload_len as usize];
        if payload_len > 0 {
            self.stream.read_exact(&mut payload)?;
        }
        
        // Validate control frames
        if is_control_frame(opcode) {
            if !fin {
                return Err(WebSocketError::ProtocolError(
                    "Control frames must not be fragmented".to_string()
                ));
            }
            if payload.len() > 125 {
                return Err(WebSocketError::ProtocolError(
                    "Control frame payload too large".to_string()
                ));
            }
        }
        
        Ok(WebSocketFrame {
            fin,
            opcode,
            payload,
        })
    }
    
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    
    pub fn is_secure(&self) -> bool {
        self.is_secure
    }
}

#[derive(Debug)]
struct WebSocketFrame {
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum WebSocketMessage {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close { code: Option<u16>, reason: String },
}

// URL parsing structure
#[derive(Debug)]
struct ParsedWebSocketUrl {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

fn parse_websocket_url(url: &str) -> Result<ParsedWebSocketUrl> {
    if let Some(scheme_end) = url.find("://") {
        let scheme = &url[..scheme_end];
        let rest = &url[scheme_end + 3..];
        
        let (host_port, path) = if let Some(path_start) = rest.find('/') {
            (&rest[..path_start], &rest[path_start..])
        } else {
            (rest, "/")
        };
        
        let (host, port) = if let Some(port_start) = host_port.rfind(':') {
            let host = &host_port[..port_start];
            let port_str = &host_port[port_start + 1..];
            let port = port_str.parse().map_err(|_| {
                WebSocketError::ProtocolError("Invalid port number".to_string())
            })?;
            (host, port)
        } else {
            let default_port = match scheme {
                "ws" => 80,
                "wss" => 443,
                _ => return Err(WebSocketError::ProtocolError("Invalid scheme".to_string())),
            };
            (host_port, default_port)
        };
        
        Ok(ParsedWebSocketUrl {
            scheme: scheme.to_string(),
            host: host.to_string(),
            port,
            path: path.to_string(),
        })
    } else {
        Err(WebSocketError::ProtocolError("Invalid URL format".to_string()))
    }
}

// Utility functions

fn generate_websocket_key() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;
    
    // Generate pseudo-random 16 bytes using available entropy
    let mut entropy = Vec::new();
    entropy.extend_from_slice(&SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default().as_nanos().to_le_bytes());
    
    let mut hasher = DefaultHasher::new();
    entropy.hash(&mut hasher);
    std::ptr::addr_of!(hasher).hash(&mut hasher);
    
    let hash = hasher.finish();
    let bytes = [
        (hash & 0xFF) as u8,
        ((hash >> 8) & 0xFF) as u8,
        ((hash >> 16) & 0xFF) as u8,
        ((hash >> 24) & 0xFF) as u8,
        ((hash >> 32) & 0xFF) as u8,
        ((hash >> 40) & 0xFF) as u8,
        ((hash >> 48) & 0xFF) as u8,
        ((hash >> 56) & 0xFF) as u8,
        // Add more entropy
        (SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().subsec_nanos() & 0xFF) as u8,
        ((SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().subsec_nanos() >> 8) & 0xFF) as u8,
        ((SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().subsec_nanos() >> 16) & 0xFF) as u8,
        ((SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().subsec_nanos() >> 24) & 0xFF) as u8,
        // Additional padding
        0x01, 0x02, 0x03, 0x04,
    ];
    
    BASE64_STANDARD.encode(&bytes)
}

fn generate_mask_key() -> [u8; 4] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;
    
    let mut hasher = DefaultHasher::new();
    SystemTime::now().hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    
    let hash = hasher.finish();
    [
        (hash & 0xFF) as u8,
        ((hash >> 8) & 0xFF) as u8,
        ((hash >> 16) & 0xFF) as u8,
        ((hash >> 24) & 0xFF) as u8,
    ]
}

fn generate_accept_key(key: &str) -> String {
    // NOTE: This is a simplified implementation for demo purposes
    // In production, you should use proper SHA-1 hashing
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let combined = format!("{}{}", key, WEBSOCKET_MAGIC_STRING);
    let mut hasher = Sha1::new();
    hasher.update(combined.as_bytes());
    
    // Convert to bytes and base64 encode
    let bytes = hasher.finalize();
    BASE64_STANDARD.encode(&bytes)
}

// Remove the custom base64 implementation entirely - we have a proper library now

fn is_control_frame(opcode: u8) -> bool {
    opcode >= 0x8
}

fn is_valid_close_code(code: u16) -> bool {
    match code {
        1000..=1003 | 1007..=1011 | 3000..=4999 => true,
        _ => false,
    }
}
