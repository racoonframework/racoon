use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::core::forms::{Files, FormConstraints, FormData};

use crate::core::headers::{HeaderValue, Headers};
use crate::core::parser::multipart::MultipartParser;
use crate::core::parser::urlencoded::UrlEncodedParser;
use crate::core::server::Context;
use crate::core::stream::Stream;

use crate::core::path::PathParams;
use crate::{racoon_debug, racoon_error};

use crate::core::cookie;
use crate::core::cookie::{parse_cookies_from_header, Cookies};
use crate::core::session::{Session, SessionManager};
use crate::core::shortcuts::SingleText;

pub type QueryParams = HashMap<String, Vec<String>>;

pub struct Request {
    pub stream: Arc<Stream>,
    context: Arc<Context>,
    pub scheme: String,
    pub method: String,
    pub path: String,
    pub http_version: u8,
    pub headers: Headers,
    pub path_params: PathParams,
    pub query_params: QueryParams,
    pub cookies: Cookies,
    pub session: Session,
    pub body_read: Arc<AtomicBool>,
    pub form_constraints: Arc<FormConstraints>,
    pub response_headers: Arc<Mutex<Headers>>,
}

impl Request {
    pub async fn from(
        stream: Arc<Stream>,
        context: Arc<Context>,
        scheme: String,
        method: String,
        path: String,
        http_version: u8,
        headers: Headers,
        path_params: PathParams,
        query_params: QueryParams,
        session_manager: Arc<SessionManager>,
        body_read: Arc<AtomicBool>,
        form_constraints: Arc<FormConstraints>,
        response_headers: Arc<Mutex<Headers>>,
    ) -> Self {
        let cookies = parse_cookies_from_header(&headers);
        let session_id = cookies.value("sessionid");

        let has_session_id = session_id.is_some();
        let session = Session::from(session_manager, session_id);

        // If sessionid was not present in cookie, puts additional Set-Cookie header in the
        // response.
        if !has_session_id {
            // New generated sessionid
            let session_id = session.session_id().await;

            {
                let mut response_headers = response_headers.lock().await;
                cookie::set_cookie(
                    &mut response_headers,
                    "sessionid",
                    session_id,
                    Duration::from_secs(7 * 86400),
                );
            }
        }

        Self {
            stream,
            context,
            scheme,
            method,
            path,
            http_version,
            headers,
            path_params,
            query_params,
            cookies,
            session,
            body_read,
            form_constraints,
            response_headers,
        }
    }

    pub async fn remote_addr(&self) -> Option<String> {
        self.stream.peer_addr().await
    }

    pub fn context<T: 'static>(&self) -> Option<&T> {
        self.context.downcast_ref::<T>()
    }

    pub async fn parse(&self) -> (FormData, Files) {
        self.parse_body(self.form_constraints.clone()).await
    }

    async fn parse_body(&self, form_constraints: Arc<FormConstraints>) -> (FormData, Files) {
        let form_data = FormData::new();
        let files = Files::new();

        let content_type;
        if let Some(value) = self.headers.value("Content-Type") {
            content_type = value;
        } else {
            racoon_debug!("Content type is missing.");
            return (form_data, files);
        }

        let body_read = self.body_read.clone();
        body_read.store(false, Ordering::Relaxed);

        if content_type
            .to_lowercase()
            .starts_with("multipart/form-data")
        {
            racoon_debug!("Parsing with MultipartParser");

            return match MultipartParser::parse(
                self.stream.clone(),
                form_constraints,
                &self.headers,
            )
            .await
            {
                Ok((form_data, files)) => {
                    self.body_read.store(true, Ordering::Relaxed);
                    (form_data, files)
                }
                Err(error) => {
                    racoon_error!("Error while parsing multipart body: {:?}", error);
                    (form_data, files)
                }
            };
        } else if content_type
            .to_lowercase()
            .starts_with("application/x-www-form-urlencoded")
        {
            racoon_debug!("Parsing with UrlEncoded parser.");
            return match UrlEncodedParser::parse(
                self.stream.clone(),
                &self.headers,
                form_constraints,
            )
            .await
            {
                Ok(form_data) => {
                    self.body_read.store(true, Ordering::Relaxed);
                    (form_data, files)
                }
                Err(error) => {
                    racoon_error!("Error while parsing x-www-urlencoded form. {:?}", error);
                    (form_data, files)
                }
            };
        }

        racoon_debug!("Unhandled enctype: {}", content_type);
        (form_data, files)
    }
}

impl Clone for Request {
    fn clone(&self) -> Self {
        Self {
            stream: self.stream.clone(),
            context: self.context.clone(),
            scheme: self.scheme.clone(),
            method: self.method.clone(),
            path: self.path.clone(),
            http_version: self.http_version.clone(),
            headers: self.headers.clone(),
            path_params: self.path_params.clone(),
            query_params: self.query_params.clone(),
            cookies: self.cookies.clone(),
            session: self.session.clone(),
            body_read: self.body_read.clone(),
            form_constraints: self.form_constraints.clone(),
            response_headers: self.response_headers.clone(),
        }
    }
}

#[derive(Debug)]
pub enum RequestError {
    HeaderSizeExceed,
    Others(String),
}
