use std::io::{Read, Write, BufRead, BufReader};
use std::net::TcpStream;
use std::collections::HashMap;
use anyhow::Result;
use base64::prelude::*;

const OPCODE_CONTINUATION: u8 = 0x0;
const OPCODE_TEXT: u8 = 0x1;
const OPCODE_BINARY: u8 = 0x2;
const OPCODE_CLOSE: u8 = 0x8;
const OPCODE_PING: u8 = 0x9;
const OPCODE_PONG: u8 = 0xA;

struct WebsocketClient {
    stream: TcpStream,
}

impl WebsocketClient {
    fn connect(host: &str, port: u16, path: &str) -> Result<Self> {
        let mut stream = TcpStream::connect(format!("{}:{}", host, port))?;

        let key = BASE64_STANDARD.encode(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);

        let request = format!(
            "GET {} HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             \r\n",
            path, host, port, key
        );

        stream.write_all(request.as_bytes())?;

        let mut reader = BufReader::new(&stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;

        if !response_line.contains("101") {
            return Err(anyhow::anyhow!("Websocket handshake failed"));
        }

        let mut headers = HashMap::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            if line.trim().is_empty() {
                break;
            }

            if let Some((key, value)) = line.split_once(": ") {
                headers.insert(key.to_lowercase(), value.trim().to_string());
            }
        }

        if headers.get("upgrade") != Some(&"websocket".to_string()) {
            return Err(anyhow::anyhow!("Upgrade header not present or incorrect"));
        }

        println!("Websocket connection established to {}:{}", host, port);

        Ok( WebsocketClient{ stream })
    }

    fn send_text(&mut self, message: &str) -> Result<()> {
        self.send_frame(OPCODE_TEXT, message.as_bytes())
    }

    fn send_binary(&mut self, data: &[u8]) -> Result<()> {
        self.send_frame(OPCODE_BINARY, data)
    }

    fn send_ping(&mut self, data: &[u8]) -> Result<()> {
        self.send_frame(OPCODE_PING, data)
    }

    fn send_pong(&mut self, data: &[u8]) -> Result<()> {
        self.send_frame(OPCODE_PONG, data)
    }

    fn send_close(&mut self) -> Result<()> {
        self.send_frame(OPCODE_CLOSE, &[])
    }

    fn send_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<()> {
        let mut frame = Vec::new();

        frame.push(0x80 | opcode); // FIN bit set and opcode

        let payload_length = payload.len();
        let mask_key = [0x12, 0x34, 0x56, 0x78]; // Example mask key, not used in this case

        if payload_length < 126 {
            frame.push(0x80 | payload_length as u8); // Mask bit set and payload length
        } else if payload_length < 65536 {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload_length as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(payload_length as u64).to_be_bytes());
        }

        frame.extend_from_slice(&mask_key);

        for (i, &byte) in payload.iter().enumerate() {
            frame.push(byte ^ mask_key[i % 4]); // Apply mask
        }

        self.stream.write_all(&frame)?;

        Ok(())
    }
}