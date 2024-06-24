pub mod frame;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use serde_json::Value;
use sha1::{Digest, Sha1};
use uuid::Uuid;

use crate::core::headers::{HeaderValue, Headers};
use crate::core::request::Request;
use crate::core::response::status::ResponseStatus;
use crate::core::response::{response_to_bytes, AbstractResponse, HttpResponse};
use crate::core::stream::Stream;
use crate::core::websocket::frame::{reader, Frame};
use crate::{racoon_debug, racoon_error};

use super::stream;

const DEFAULT_MAX_PAYLOAD_SIZE: u64 = 5 * 1024 * 1024; // 5 MiB

pub enum Message {
    Continue(Vec<u8>),
    Text(String),
    Binary(Vec<u8>),
    Close(u16, String),
    Ping(),
    Pong(),
    Others(Vec<u8>),
}

pub struct WebSocket {
    pub uid: String,
    stream: Arc<Stream>,
    request_validated: bool,
    receive_next: Arc<AtomicBool>,
    headers: Headers,
    body: Vec<u8>,
}

impl Clone for WebSocket {
    fn clone(&self) -> Self {
        Self {
            uid: self.uid.clone(),
            stream: self.stream.clone(),
            request_validated: self.request_validated.clone(),
            receive_next: self.receive_next.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
        }
    }
}

impl AbstractResponse for WebSocket {
    fn status(&self) -> (u32, String) {
        (200, "OK".to_string())
    }

    fn serve_default(&mut self) -> bool {
        false
    }

    fn get_headers(&mut self) -> &mut Headers {
        &mut self.headers
    }

    fn get_body(&mut self) -> &mut Vec<u8> {
        &mut self.body
    }

    fn should_close(&mut self) -> bool {
        true
    }
}

impl WebSocket {
    pub async fn from(request: &Request) -> (Self, bool) {
        Self::from_opt(request, true).await
    }

    pub async fn from_opt(request: &Request, periodic_ping: bool) -> (Self, bool) {
        let instance = match WebSocket::validate(request).await {
            Ok(instance) => instance,
            Err(error) => {
                racoon_error!("WS Error: {}", error);

                let failed = Self {
                    uid: Uuid::new_v4().to_string(),
                    stream: request.stream.clone(),
                    request_validated: false,
                    receive_next: Arc::new(AtomicBool::new(true)),
                    headers: Headers::new(),
                    body: Vec::new(),
                };
                return (failed, false);
            }
        };

        if periodic_ping {
            instance.ping_with_interval(Duration::from_secs(10)).await;
        }

        (instance, true)
    }

    async fn validate(request: &Request) -> Result<Self, String> {
        if request.method != "GET" {
            return Err("Invalid request method.".to_owned());
        }

        // Validate connection header
        if let Some(value) = request.headers.value("Connection") {
            // Connection header can contain multiple values seperated by comma.
            // Checks if 'upgrade' is specified or not. If not returns error.
            if !value.to_lowercase().contains("upgrade") {
                return Err("Connection header does not specify to upgrade".to_string());
            }
        } else {
            return Err("Connection header is missing.".to_string());
        }

        let upgrade;
        if let Some(value) = request.headers.value("Upgrade") {
            upgrade = value;
        } else {
            return Err("Upgrade header is missing.".to_string());
        };

        let sec_websocket_key;
        if let Some(value) = request.headers.value("Sec-WebSocket-Key") {
            // According to RFC, any leading or trailing spaces must be removed.
            sec_websocket_key = value.trim().to_string();
        } else {
            return Err("Sec-WebSocket-Key header is missing".to_string());
        }

        if upgrade.to_lowercase() == "websocket" {
        } else {
            return Err("Upgrade header is not set to websocket.".to_string());
        }

        let instance = Self {
            uid: Uuid::new_v4().to_string(),
            stream: request.stream.clone(),
            request_validated: true,
            receive_next: Arc::new(AtomicBool::new(false)),
            headers: Headers::new(),
            body: Vec::new(),
        };

        match Self::handshake(request.stream.clone(), &sec_websocket_key).await {
            Ok(()) => {}
            Err(error) => {
                return Err(format!("Failed to handshake. {}", error));
            }
        };

        instance.receive_next.store(true, Ordering::Relaxed);
        Ok(instance)
    }

    ///
    /// More information: <https://datatracker.ietf.org/doc/html/rfc6455#section-1.3>
    ///
    async fn handshake(stream: Arc<Stream>, sec_websocket_key: &str) -> std::io::Result<()> {
        let base64_hash = Self::handshake_key_base64(sec_websocket_key);

        let mut http_response = HttpResponse::switching_protocols();
        let headers = http_response.get_headers();
        headers.set("Connection", "upgrade");
        headers.set("Upgrade", "websocket");
        headers.set("Sec-WebSocket-Accept", base64_hash.as_bytes());

        let mut response: Box<dyn AbstractResponse> = http_response.empty();
        let response_bytes = response_to_bytes(&mut response);
        Ok(stream.write_chunk(&response_bytes).await?)
    }

    fn handshake_key_base64(sec_websocket_key: &str) -> String {
        // WebSocket GUID constant
        const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
        let new_key = format!("{}{}", sec_websocket_key.trim(), WEBSOCKET_GUID);

        // Generates Sha1 hash
        let mut hasher = Sha1::new();
        hasher.update(new_key);
        let hash_result = hasher.finalize().to_vec();

        // Encodes to base 64
        base64::engine::general_purpose::STANDARD.encode(hash_result)
    }

    async fn ping_with_interval(&self, duration: Duration) {
        let stream = self.stream.clone();
        let receive_next = self.receive_next.clone();

        tokio::spawn(async move {
            racoon_debug!("Sending periodic ping frames...");

            let mut interval = tokio::time::interval(duration);

            // More information: https://datatracker.ietf.org/doc/html/rfc6455#section-5.5.2
            let frame = Frame {
                fin: 1,
                op_code: 9,
                payload: vec![],
            };

            let bytes = frame::builder::build(&frame);
            interval.tick().await;

            loop {
                interval.tick().await;
                racoon_debug!("Sending ping...");

                match stream.write_chunk(&bytes).await {
                    Ok(()) => {}
                    Err(error) => {
                        // Ping failed, so if messages are waiting, stops waiting new messages.
                        receive_next.store(false, Ordering::Relaxed);
                        racoon_debug!("Ping failed. Error: {}", error);
                        break;
                    }
                }
            }
        });
    }

    async fn send_pong(&self) {
        racoon_debug!("Sending pong frame.");

        // More information: https://datatracker.ietf.org/doc/html/rfc6455#section-5.5.2
        let frame = Frame {
            fin: 1,
            op_code: 10,
            payload: vec![],
        };

        let bytes = frame::builder::build(&frame);
        match self.stream.write_chunk(&bytes).await {
            Ok(()) => {}
            Err(error) => {
                // Pong failed, so stops receiving messages.
                self.receive_next.store(false, Ordering::Relaxed);
                racoon_debug!("Pong failed. Error: {}", error);
            }
        }
    }

    pub async fn receive_message_with_limit(&mut self, max_payload_size: u64) -> Option<Message> {
        if !self.receive_next.load(Ordering::Relaxed) {
            return None;
        };

        let mut response: Vec<u8> = vec![];

        loop {
            let frame = match reader::read_frame(self.stream.clone(), max_payload_size).await {
                Ok(frame) => frame,
                Err(error) => {
                    // Stops waiting for new messages
                    self.receive_next.store(false, Ordering::Relaxed);
                    return Some(Message::Close(1000, error.to_string()));
                }
            };

            response.extend(&frame.payload);

            // Checks response size
            if response.len() > DEFAULT_MAX_PAYLOAD_SIZE as usize {
                return Some(Message::Close(0, "Max payload size exceed.".to_string()));
            }

            // If fin is 1, the complete message is received.
            if frame.fin == 1 {
                return if frame.op_code == 0 {
                    Some(Message::Continue(frame.payload))
                } else if frame.op_code == 1 {
                    // Text Frame
                    let payload_text = String::from_utf8_lossy(frame.payload.as_slice());
                    Some(Message::Text(payload_text.to_string()))
                } else if frame.op_code == 2 {
                    // Binary frame
                    Some(Message::Binary(frame.payload))
                } else if frame.op_code == 8 {
                    // Connection close frame
                    self.receive_next.store(false, Ordering::Relaxed);
                    let close_code = self.close_code_from_payload(&frame.payload);
                    let close_message = self.close_message_from_payload(&frame.payload);
                    Some(Message::Close(close_code, close_message))
                } else if frame.op_code == 9 {
                    // Ping frame
                    self.send_pong().await;
                    Some(Message::Ping())
                } else if frame.op_code == 10 {
                    // Pong frame
                    Some(Message::Pong())
                } else {
                    Some(Message::Others(frame.payload))
                };
            }
        }
    }

    pub async fn message(&mut self) -> Option<Message> {
        self.receive_message_with_limit(DEFAULT_MAX_PAYLOAD_SIZE)
            .await
    }

    pub async fn send_text<S: AsRef<str>>(&self, message: S) -> std::io::Result<()> {
        let message = message.as_ref();

        let frame = Frame {
            fin: 1,
            op_code: 1,
            payload: message.as_bytes().to_vec(),
        };

        let bytes = frame::builder::build(&frame);
        self.stream.write_chunk(&bytes).await?;
        Ok(())
    }

    pub async fn send_bytes<B: AsRef<[u8]>>(&self, bytes: B) -> std::io::Result<()> {
        let payload = Vec::from(bytes.as_ref());

        let frame = Frame {
            fin: 1,
            op_code: 2,
            payload,
        };

        let bytes = frame::builder::build(&frame);
        self.stream.write_chunk(&bytes).await?;

        Ok(())
    }

    pub async fn send_json(&self, json: &Value) -> std::io::Result<()> {
        self.send_text(json.to_string().as_str()).await
    }

    pub async fn bad_request(self) -> Box<Self> {
        let mut response: Box<dyn AbstractResponse> =
            HttpResponse::bad_request().body("Bad Request");
        let response_bytes = response_to_bytes(&mut response);
        let _ = self.stream.write_chunk(&response_bytes).await;
        Box::new(self)
    }

    pub fn exit(self) -> Box<Self> {
        Box::new(self)
    }

    fn close_code_from_payload(&self, response: &[u8]) -> u16 {
        if response.len() == 2 {
            let mut tmp_bytes = [0u8; 2];
            tmp_bytes.copy_from_slice(response);
            return u16::from_be_bytes(tmp_bytes);
        }

        racoon_debug!(
            "Close payload length expected more than 2. But found: {}",
            response.len()
        );
        return 0;
    }

    fn close_message_from_payload(&self, response: &[u8]) -> String {
        if response.len() < 3 {
            return "No close message specified.".to_string();
        }

        let message_bytes = &response[2..];
        String::from_utf8_lossy(&message_bytes).to_string()
    }
}
