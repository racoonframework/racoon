//!
//! Racoon is a fast, fully customizable web framework for Rust focusing on simplicity.
//!
//! To use Racoon, you need minimal Rust version 1.75.0 and Tokio runtime.
//!
//! Getting started:
//! ```rust,no_run
//! use racoon::core::path::Path;
//! use racoon::core::request::Request;
//! use racoon::core::response::{HttpResponse, Response};
//! use racoon::core::response::status::ResponseStatus;
//! use racoon::core::server::Server;
//!
//! use racoon::view;
//!
//! async fn home(request: Request) -> Response {
//!     HttpResponse::ok().body("Home")
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let paths = vec![
//!         Path::new("/", view!(home))
//!     ];
//!
//!     let result = Server::bind("127.0.0.1:8080")
//!         .urls(paths)
//!        .run().await;
//!
//!     println!("Failed to run server: {:?}", result);
//! }
//! ```
//!

pub mod core;
pub mod forms;
pub mod prelude;
