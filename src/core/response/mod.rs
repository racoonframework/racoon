pub mod status;

use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;

use crate::core::cookie;
use crate::core::headers::{HeaderValue, Headers};
use crate::core::response::status::ResponseStatus;

pub trait AbstractResponse: Send {
    fn status(&self) -> (u32, String);
    fn serve_default(&mut self) -> bool;
    fn get_headers(&mut self) -> &mut Headers;
    fn get_body(&mut self) -> &mut Vec<u8>;
    fn should_close(&mut self) -> bool;
}

pub type Response = Box<dyn AbstractResponse>;

pub struct HttpResponse {
    status_code: u32,
    status_text: String,
    headers: Headers,
    body: Vec<u8>,
    keep_alive: bool,
    serve_default: bool,
}

impl AbstractResponse for HttpResponse {
    fn status(&self) -> (u32, String) {
        (self.status_code, self.status_text.to_owned())
    }

    fn serve_default(&mut self) -> bool {
        self.serve_default
    }

    fn get_headers(&mut self) -> &mut Headers {
        &mut self.headers
    }

    fn get_body(&mut self) -> &mut Vec<u8> {
        &mut self.body
    }

    fn should_close(&mut self) -> bool {
        !self.keep_alive
    }
}

impl HttpResponse {
    pub fn content_type(mut self, value: &str) -> Self {
        self.headers.set("Content-Type", value.as_bytes());
        self
    }

    pub fn keep_alive(mut self, is_alive: bool) -> Self {
        self.keep_alive = !is_alive;
        self
    }

    pub fn disable_serve_default(mut self) -> Self {
        self.serve_default = false;
        self
    }

    pub fn location(mut self, url: &str) -> Box<Self> {
        self.get_headers().set("Location", url);
        Box::new(self)
    }

    pub fn body<S: AsRef<str>>(mut self, data: S) -> Box<Self> {
        let data = data.as_ref();

        self.headers
            .set("Content-Length", data.len().to_string());

        self.headers.set("Content-Type", "text/html");

        if self.headers.value("Connection").is_none() {
            if self.keep_alive {
                self.headers.set("Connection", "keep-alive");
            } else {
                self.headers.set("Connection", "close");
            }
        }

        self.body = data.as_bytes().to_vec();

        Box::new(self)
    }

    pub fn empty(self) -> Box<Self> {
        self.body("")
    }

    pub fn set_cookie<S: AsRef<str>>(&mut self, name: S, value: S, max_age: Duration) {
        let headers = self.get_headers();
        cookie::set_cookie(headers, name, value, max_age);
    }

    pub fn remove_cookie<S: AsRef<str>>(&mut self, name: S) {
        let headers = &mut self.headers;
        let expire_header_value = format!(
            "{}=;Expires=Sun, 06 Nov 1994 08:49:37 GMT; Path=/",
            name.as_ref()
        );
        headers.set_multiple("Set-Cookie", expire_header_value);
    }
}

impl ResponseStatus for HttpResponse {
    fn with_status(status_code: u32, status_text: &str) -> Self {
        Self {
            status_code,
            status_text: status_text.to_owned(),
            headers: HashMap::new(),
            body: vec![],
            keep_alive: true,
            serve_default: true,
        }
    }
}

pub fn response_to_bytes(response: &mut Box<dyn AbstractResponse>) -> Vec<u8> {
    let mut response_bytes: Vec<u8> = Vec::with_capacity(response.get_body().len());
    let (status_code, status_text) = response.status();

    // Append header response start line
    let response_header_begin = format!("HTTP/1.1 {} {}\r\n", status_code, status_text);
    response_bytes.extend(response_header_begin.as_bytes());

    // Append headers
    response.get_headers().iter().for_each(|(name, values)| {
        for value in values {
            response_bytes.extend(name.as_bytes());
            response_bytes.extend(b": ");
            response_bytes.extend(value);
            response_bytes.extend(b"\r\n");
        }
    });

    response_bytes.extend(b"\r\n");

    // Body start
    response_bytes.extend(response.get_body().as_slice());
    response_bytes
}

pub struct JsonResponse {
    http_response: HttpResponse,
}

impl JsonResponse {
    pub fn body(mut self, json: serde_json::Value) -> Box<Self> {
        let json_text = json.to_string();

        self.http_response
            .headers
            .set("Content-Length", json_text.len().to_string().as_bytes());

        if self.http_response.headers.value("Connection").is_none() {
            if self.http_response.keep_alive {
                self.http_response
                    .headers
                    .set("Connection", "keep-alive");
            } else {
                self.http_response
                    .headers
                    .set("Connection", "close");
            }
        }

        self.http_response.body = json_text.as_bytes().to_vec();
        Box::new(self)
    }

    ///
    /// Creates empty JSON object response.
    ///
    pub fn empty(self) -> Box<Self> {
        self.body(json!({}))
    }

    ///
    /// Sets cookie in max age from "/" path.
    ///
    pub fn set_cookie<S: AsRef<str>>(&mut self, name: S, value: S, max_age: Duration) {
        self.http_response.set_cookie(name, value, max_age);
    }

    ///
    /// Removes cookie from "/" path.
    ///
    pub fn remove_cookie<S: AsRef<str>>(&mut self, name: S) {
        self.http_response.remove_cookie(name)
    }
}

impl AbstractResponse for JsonResponse {
    fn status(&self) -> (u32, String) {
        self.http_response.status()
    }

    fn serve_default(&mut self) -> bool {
        self.http_response.serve_default
    }

    fn get_headers(&mut self) -> &mut Headers {
        self.http_response.get_headers()
    }

    fn get_body(&mut self) -> &mut Vec<u8> {
        self.http_response.get_body()
    }

    fn should_close(&mut self) -> bool {
        self.http_response.should_close()
    }
}

impl ResponseStatus for JsonResponse {
    fn with_status(status_code: u32, status_text: &str) -> Self {
        let mut http_response = HttpResponse::with_status(status_code, status_text);
        let headers = http_response.get_headers();
        headers.set("Content-Type", "application/json");

        Self { http_response }
    }
}
