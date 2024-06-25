# Racoon

<p align="center" style="text-align: center;">
    <img src="logo.png" width="300">
    <br>
    <a href="https://github.com/racoonframework/racoon/actions/workflows/build.yml">
        <img src="https://github.com/racoonframework/racoon/actions/workflows/build.yml/badge.svg">
    </a>
</p>


Racoon is a fast, fully customizable web framework for Rust focusing on simplicity.

[Learn Racoon](https://racoonframework.github.io)

## Installation

You will need `tokio` runtime to run Racoon. Run `cargo add tokio` to install tokio crate.

```
[dependencies]
racoon = "0.1.1"
```

## Basic Usage

```rust
use racoon::core::path::Path;
use racoon::core::request::Request;
use racoon::core::response::{HttpResponse, Response};
use racoon::core::response::status::ResponseStatus;
use racoon::core::server::Server;

use racoon::view;

async fn home(request: Request) -> Response {
    HttpResponse::ok().body("Home")
}

#[tokio::main]
async fn main() {
    let paths = vec![
        Path::new("/", view!(home))
    ];

    let result = Server::bind("127.0.0.1:8080")
        .urls(paths)
        .run().await;

    println!("Failed to run server: {:?}", result);
}
```

### File Handling

There are multiple ways to handle files in Racoon. The simple way is to use `request.parse()` method.

```rust
use racoon::core::request::Request;
use racoon::core::response::{HttpResponse, Response};
use racoon::core::response::status::ResponseStatus;
use racoon::core::forms::FileField;
use racoon::core::shortcuts::SingleText;

async fn upload_form(request: Request) -> Response {
    if request.method == "POST" {
        // Parses request body
        let (form_data, files) = request.parse().await;
        println!("Name: {:?}", form_data.value("name"));

        let file = files.value("file");
        println!("File: {:?}", file);
        return HttpResponse::ok().body("Uploaded");
    }

    HttpResponse::bad_request().body("Use POST method to upload file.")
}
```

## WebSocket example

```rust
use racoon::core::path::Path;
use racoon::core::request::Request;
use racoon::core::response::Response;
use racoon::core::server::Server;
use racoon::core::websocket::{Message, WebSocket};

use racoon::view;

async fn ws(request: Request) -> Response {
    let (websocket, connected) = WebSocket::from(&request).await;
    if !connected {
        // WebSocket connection didn't success
        return websocket.bad_request().await;
    }

    println!("WebSocket client connected.");

    // Receive incoming messages
    while let Some(message) = websocket.message().await {
        match message {
            Message::Text(text) => {
                println!("Message: {}", text);

                // Sends received message back
                let _ = websocket.send_text(text.as_str()).await;
            }
            _ => {}
        }
    }
    websocket.exit()
}

#[tokio::main]
async fn main() {
    let paths = vec![
        Path::new("/ws/", view!(ws))
        ];

    let _ = Server::bind("127.0.0.1:8080")
            .urls(paths)
            .run().await;
}
```


## Benchmark

```shell
wrk -c8 -d4s -t4 http://127.0.0.1:8080
```

Result on AMD Ryzen 5 7520U with Radeon Graphics.

```text
Running 4s test @ http://127.0.0.1:8080
  4 threads and 8 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency    50.62us   60.12us   2.52ms   97.98%
    Req/Sec    40.59k     4.10k   44.20k    92.68%
  662179 requests in 4.10s, 46.73MB read
Requests/sec: 161510.26
Transfer/sec:     11.40MB
```

This benchmark does not make sense in real world.

