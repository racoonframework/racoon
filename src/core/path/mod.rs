use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::core::request::Request;
use crate::core::response::status::ResponseStatus;
use crate::core::response::{AbstractResponse, HttpResponse, Response};
use crate::core::shortcuts::SingleText;

use super::headers::HeaderValue;

pub type View = fn(Request) -> Pin<Box<dyn Future<Output = Box<dyn AbstractResponse>> + Send>>;

pub struct Path {
    pub name: String,
    pub view: View,
}

impl Path {
    pub fn new<S: AsRef<str>>(name: S, view: View) -> Self {
        Self {
            name: name.as_ref().to_string(),
            view,
        }
    }

    pub async fn resolve(request: Request, view: Option<View>) -> Response {
        let mut response;
        let response_headers_from_request_ref = request.response_headers.clone();

        if let Some(view) = view {
            response = view(request).await;
        } else {
            response = HttpResponse::not_found().body("404 Page not found");
        }

        // Adds additional headers received from request struct.

        let response_headers_from_request = response_headers_from_request_ref.lock().await;
        let response_headers = response.get_headers();

        for (name, values) in response_headers_from_request.iter() {
            for value in values {
                response_headers.set_multiple(name, value);
            }
        }
        response
    }
}

impl Clone for Path {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            view: self.view.clone(),
        }
    }
}

pub type Paths = Vec<Path>;

#[derive(Debug)]
pub struct PathParams {
    params: HashMap<String, String>,
}

impl Clone for PathParams {
    fn clone(&self) -> Self {
        Self {
            params: self.params.clone(),
        }
    }
}

impl SingleText for PathParams {
    fn value<S: AsRef<str>>(&self, name: S) -> Option<&String> {
        let name = name.as_ref();
        self.params.get(name)
    }
}

impl PathParams {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: &str, value: &str) {
        self.params.insert(key.to_owned(), value.to_owned());
    }

    pub fn map(&mut self) -> &mut HashMap<String, String> {
        &mut self.params
    }
}

#[macro_export]
macro_rules! view {
    ($view_name: ident) => {
        |request: racoon::core::request::Request| Box::pin($view_name(request))
    };
}
