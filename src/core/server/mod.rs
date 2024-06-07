pub mod utils;

use std::any::Any;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use matchit::Router;

use tokio::net::{TcpListener, UnixListener};
use tokio::sync::Mutex;
use tokio_rustls::TlsAcceptor;

use crate::core::forms::FormConstraints;
use crate::core::headers::HeaderValue;
use crate::core::middleware::Middleware;
use crate::core::parser::headers::read_request_headers;
use crate::core::parser::{params, path};
use crate::core::path::{Path, PathParams, Paths};
use crate::core::request::Request;
use crate::core::response::status::ResponseStatus;
use crate::core::response::{AbstractResponse, HttpResponse};
use crate::core::stream::{Stream, TcpStreamWrapper, UnixStreamWrapper};

use crate::{racoon_debug, racoon_error};

use crate::core::headers::Headers;
use crate::core::response;
use crate::core::session::managers::FileSessionManager;
use crate::core::session::{AbstractSessionManager, SessionManager};
use crate::core::stream::TlsTcpStreamWrapper;

pub struct RequestConstraints {
    pub max_request_header_size: usize,
    pub max_header_count: usize,
}

impl RequestConstraints {
    pub fn max_request_header_size(&self, buffer_size: usize) -> usize {
        if buffer_size > self.max_request_header_size {
            return buffer_size;
        }

        self.max_request_header_size
    }
}

pub type Context = Pin<Box<dyn Any + Send + Sync>>;

#[derive(Debug)]
pub enum RequestScheme {
    HTTP,
    HTTPS,
}

pub struct Server {
    scheme: String,
    bind_address: Option<String>,
    sock_path: Option<String>,
    custom_tcp_listener: Option<TcpListener>,
    custom_unix_listener: Option<UnixListener>,
    tls_acceptor: Option<TlsAcceptor>,
    router: Arc<Router<Path>>,
    context: Arc<Context>,
    buffer_size: usize,
    middleware: Option<Middleware>,
    request_constraints: Arc<RequestConstraints>,
    form_constraints: Arc<FormConstraints>,
    session_manager: Option<Arc<SessionManager>>,
}

impl Server {
    fn initialize_default() -> Self {
        let default_request_constraint = RequestConstraints {
            max_request_header_size: 5 * 1024 * 1024, // 5 MiB
            max_header_count: 100,
        };

        let default_form_constraint = FormConstraints::new(
            512 * 1024 * 1024, // 512 MiB
            2 * 1024,          // 2 KiB
            512 * 1024 * 1024, // 512 MiB
            2 * 1024 * 1024,   // 2 MiB
            HashMap::new(),
        );

        Self {
            scheme: "http".to_string(),
            bind_address: None,
            sock_path: None,
            custom_tcp_listener: None,
            custom_unix_listener: None,
            tls_acceptor: None,
            router: Arc::new(Router::new()),
            context: Arc::new(Box::pin(None::<String>)),
            buffer_size: 8096,
            middleware: None,
            request_constraints: Arc::from(default_request_constraint),
            form_constraints: Arc::from(default_form_constraint),
            session_manager: None,
        }
    }

    /// Binds server to given port.
    pub fn bind<S: AsRef<str>>(address: S) -> Self {
        let mut instance = Self::initialize_default();
        instance.bind_address = Some(address.as_ref().to_string());
        instance
    }

    /// Binds server to Unix Domain Socket.
    pub fn bind_uds<S: AsRef<str>>(path: S) -> Self {
        let path = path.as_ref();

        // If sock file exists, removes sock file.
        let path_buf = PathBuf::from(path);
        if path_buf.exists() {
            let _ = std::fs::remove_file(path);
        }

        let mut instance = Self::initialize_default();
        instance.sock_path = Some(path.to_string());
        instance
    }

    pub fn from_tcp_listener(tcp_listener: TcpListener) -> Self {
        let mut instance = Self::initialize_default();
        instance.custom_tcp_listener = Some(tcp_listener);
        instance
    }

    pub fn from_unix_listener(unix_listener: UnixListener) -> Self {
        let mut instance = Self::initialize_default();
        instance.custom_unix_listener = Some(unix_listener);
        instance
    }

    pub fn bind_tls_custom(tcp_listener: TcpListener, tls_acceptor: TlsAcceptor) -> Self {
        let mut instance = Self::initialize_default();
        instance.custom_tcp_listener = Some(tcp_listener);
        instance.tls_acceptor = Some(tls_acceptor);
        instance.scheme = "https".to_string();
        instance
    }

    pub fn bind_tls<S: AsRef<str>>(
        address: S,
        certificate_path: S,
        private_key_path: S,
    ) -> std::io::Result<Self> {
        let acceptor = utils::tls_acceptor_from_path(certificate_path, private_key_path)?;
        let mut instance = Server::initialize_default();
        instance.scheme = "https".to_string();
        instance.bind_address = Some(address.as_ref().to_string());
        instance.tls_acceptor = Some(acceptor);
        Ok(instance)
    }

    /// Force provided scheme in all the requests
    ///
    /// # Examples
    /// ```
    /// use racoon::core::server::Server;
    /// use racoon::core::request::Request;
    /// use racoon::core::response::Response;
    /// use racoon::core::server::RequestScheme;
    ///
    ///
    /// async fn home(request: Request) -> Response {
    ///    let scheme = request.scheme;
    ///    assert_eq!(scheme, "https");
    ///
    ///    todo!()
    /// }
    ///
    /// let server = Server::bind("127.0.0.1::8080")
    ///     .set_scheme(RequestScheme::HTTPS);
    ///
    /// ```
    pub fn set_scheme(&mut self, scheme: RequestScheme) -> &mut Self {
        match scheme {
            RequestScheme::HTTP => {
                self.scheme = "http".to_string();
            }

            RequestScheme::HTTPS => {
                self.scheme = "https".to_string();
            }
        }

        self
    }

    /// Enables logging for internal debug
    pub fn enable_logging() {
        env::set_var("RACOON_LOGGING", "true");
    }

    pub fn set_session_manager<T: AbstractSessionManager + 'static>(
        &mut self,
        session_manager: T,
    ) -> &mut Self {
        self.session_manager = Some(Arc::new(Box::new(session_manager)));
        self
    }

    /// Shared context to share among views.
    pub fn context<T: Send + Sync + 'static>(&mut self, data: T) -> &mut Self {
        self.context = Arc::new(Box::pin(data));
        self
    }

    /// Buffer size for reading and writing stream.
    pub fn buffer_size(&mut self, size: usize) -> &mut Self {
        self.buffer_size = size;
        self
    }

    /// Constraints for parsing request header.
    pub fn request_constraints(&mut self, request_constraints: RequestConstraints) -> &mut Self {
        self.request_constraints = Arc::from(request_constraints);
        self
    }

    /// Constraints for parsing request body.
    pub fn form_constraints(&mut self, form_constraints: FormConstraints) -> &mut Self {
        self.form_constraints = Arc::from(form_constraints);
        self
    }

    /// Pass vec of paths.
    pub fn urls(&mut self, paths: Paths) -> &mut Self {
        let mut router = Router::new();

        for path in paths {
            let path_name = path.name.to_string();

            match router.insert(&path_name, path) {
                Ok(()) => {}
                Err(error) => {
                    panic!("Invalid path \"{}\" pattern. Error: {}", path_name, error);
                }
            }
        }
        self.router = Arc::from(router);
        self
    }

    /// Pass middleware view to capture request and response.
    pub fn wrap(&mut self, middleware: Middleware) -> &mut Self {
        self.middleware = Some(middleware);
        self
    }

    /// Runs server in blocking thread.
    pub async fn run(&mut self) -> std::io::Result<()> {
        let session_manager: Arc<SessionManager>;
        if let Some(custom_session_manager) = &self.session_manager {
            session_manager = custom_session_manager.clone();
        } else {
            session_manager = Arc::new(Box::new(FileSessionManager::new().await?));
        }

        if let Some(bind_address) = &self.bind_address {
            if self.tls_acceptor.is_some() {
                log::info!("Server listening at https://{}", bind_address);
            } else {
                log::info!("Server listening at at http://{}", bind_address);
            }

            let mut listener = TcpListener::bind(bind_address).await?;

            // If TLS acceptor is set, server will receive on HTTPS else HTTP
            Self::listen_port(
                &self.scheme,
                &mut listener,
                self.tls_acceptor.clone(),
                self.context.clone(),
                self.router.clone(),
                self.buffer_size.clone(),
                self.middleware,
                self.request_constraints.clone(),
                self.form_constraints.clone(),
                session_manager.clone(),
            )
            .await?;
        }

        if let Some(sock_path) = &self.sock_path {
            log::info!("Running is server at {}", sock_path);

            let mut listener = UnixListener::bind(sock_path)?;

            Self::listen_uds(
                &self.scheme,
                &mut listener,
                self.context.clone(),
                self.router.clone(),
                self.buffer_size.clone(),
                self.middleware,
                self.request_constraints.clone(),
                self.form_constraints.clone(),
                session_manager.clone(),
            )
            .await?;
        }

        if let Some(tls_acceptor) = &self.tls_acceptor {
            let listener = self
                .custom_tcp_listener
                .as_mut()
                .expect("Tcp Listener not set.");

            Self::listen_port(
                &self.scheme,
                listener,
                Some(tls_acceptor.clone()),
                self.context.clone(),
                self.router.clone(),
                self.buffer_size.clone(),
                self.middleware,
                self.request_constraints.clone(),
                self.form_constraints.clone(),
                session_manager.clone(),
            )
            .await?;
        }

        if let Some(listener) = self.custom_tcp_listener.as_mut() {
            Self::listen_port(
                &self.scheme,
                listener,
                None,
                self.context.clone(),
                self.router.clone(),
                self.buffer_size.clone(),
                self.middleware,
                self.request_constraints.clone(),
                self.form_constraints.clone(),
                session_manager.clone(),
            )
            .await?;
        }

        if let Some(listener) = self.custom_unix_listener.as_mut() {
            Self::listen_uds(
                &self.scheme,
                listener,
                self.context.clone(),
                self.router.clone(),
                self.buffer_size.clone(),
                self.middleware,
                self.request_constraints.clone(),
                self.form_constraints.clone(),
                session_manager.clone(),
            )
            .await?;
        }

        Ok(())
    }

    async fn listen_port(
        scheme: &String,
        listener: &mut TcpListener,
        tls_acceptor: Option<TlsAcceptor>,
        context: Arc<Context>,
        router: Arc<Router<Path>>,
        buffer_size: usize,
        middleware: Option<Middleware>,
        request_constraints: Arc<RequestConstraints>,
        form_constraints: Arc<FormConstraints>,
        session_manager: Arc<SessionManager>,
    ) -> std::io::Result<()> {
        loop {
            let router = router.clone();
            let context = context.clone();
            let tls_acceptor = tls_acceptor.clone();

            let (tcp_stream, _) = match listener.accept().await {
                Ok(result) => result,
                Err(error) => {
                    racoon_error!("Failed to accept connection: {}", error);
                    continue;
                }
            };

            let request_constraints = request_constraints.clone();
            let form_constraints = form_constraints.clone();
            let scheme = scheme.clone();
            let session_type = session_manager.clone();

            let _ = tokio::spawn(async move {
                if let Some(tls_acceptor) = tls_acceptor.clone() {
                    // With TLS
                    match TlsTcpStreamWrapper::from(tcp_stream, &tls_acceptor, buffer_size.clone())
                        .await
                    {
                        Ok(tls_tcp_stream_wrapper) => {
                            let stream = Box::new(tls_tcp_stream_wrapper);
                            Self::handle_stream(
                                stream,
                                scheme.clone(),
                                context,
                                router,
                                middleware,
                                request_constraints,
                                form_constraints,
                                session_type,
                            )
                            .await;
                        }

                        Err(error) => {
                            racoon_error!("Failed to handle accepted connection: Error: {}", error);
                        }
                    }
                } else {
                    // Without TLS
                    match TcpStreamWrapper::from(tcp_stream, buffer_size.clone()) {
                        Ok(tcp_stream_wrapper) => {
                            let stream = Box::new(tcp_stream_wrapper);
                            Self::handle_stream(
                                stream,
                                scheme,
                                context,
                                router,
                                middleware,
                                request_constraints,
                                form_constraints,
                                session_type,
                            )
                            .await;
                        }

                        Err(error) => {
                            log::error!("Failed to handle accepted connection: Error: {}", error);
                        }
                    }
                }
            });
        }
    }

    async fn listen_uds(
        scheme: &String,
        listener: &mut UnixListener,
        context: Arc<Context>,
        router: Arc<Router<Path>>,
        buffer_size: usize,
        middleware: Option<Middleware>,
        request_constraints: Arc<RequestConstraints>,
        form_constraints: Arc<FormConstraints>,
        session_type: Arc<SessionManager>,
    ) -> std::io::Result<()> {
        loop {
            let router = router.clone();
            let context = context.clone();

            let unix_stream = match listener.accept().await {
                Ok((unix_stream, _)) => unix_stream,
                Err(error) => {
                    racoon_error!("Failed to accept connection: {}", error);
                    continue;
                }
            };

            let request_constraints = request_constraints.clone();
            let form_constraints = form_constraints.clone();
            let scheme = scheme.clone();
            let session_type = session_type.clone();

            let _ = tokio::spawn(async move {
                match UnixStreamWrapper::from(unix_stream, buffer_size.clone()) {
                    Ok(unix_stream_wrapper) => {
                        let stream = Box::new(unix_stream_wrapper);

                        Self::handle_stream(
                            stream,
                            scheme,
                            context,
                            router,
                            middleware,
                            request_constraints,
                            form_constraints,
                            session_type,
                        )
                        .await;
                    }

                    Err(error) => {
                        log::error!("Failed to handle accepted connection: Error: {}", error);
                    }
                }
            });
        }
    }

    async fn handle_stream(
        stream: Stream,
        scheme: String,
        context: Arc<Context>,
        router: Arc<Router<Path>>,
        middleware: Option<Middleware>,
        request_constraints: Arc<RequestConstraints>,
        form_constraints: Arc<FormConstraints>,
        session_type: Arc<SessionManager>,
    ) {
        let stream = Arc::new(stream);

        loop {
            let request_result =
                match read_request_headers(stream.clone(), request_constraints.clone()).await {
                    Ok(result) => result,
                    Err(error) => {
                        racoon_debug!("Failed to parse request. Error: {:?}", error);
                        let mut bad_request: Box<dyn AbstractResponse> =
                            HttpResponse::request_header_fields_too_large()
                                .body("Request header too large.");

                        let response_bytes = response::response_to_bytes(&mut bad_request);
                        let _ = stream.write_chunk(&response_bytes).await;
                        let _ = stream.shutdown().await;
                        break;
                    }
                };

            let request_method;
            if let Some(method) = request_result.method {
                request_method = method;
            } else {
                racoon_debug!("Request method is missing.");
                break;
            }

            let http_version;
            if let Some(version) = request_result.http_version {
                http_version = version;
            } else {
                racoon_debug!("HTTP version is missing.");
                return;
            }

            let raw_path;
            let path;
            let query_params;

            if let Some(raw_path_value) = request_result.raw_path {
                raw_path = raw_path_value;
                let (path_value, _) = path::path_and_raw_query(&raw_path);
                path = path_value;
                query_params = params::query_params_from_raw(&raw_path);
            } else {
                racoon_debug!("Path is missing from the headers.");
                break;
            }

            let route = router.clone();
            let matched_route = match route.at(&path) {
                Ok(matched) => Some(matched),
                Err(_) => None,
            };

            let mut params = PathParams::new();
            let view;
            if let Some(route) = matched_route {
                view = Some(route.value.view);
                route.params.iter().for_each(|(key, value)| {
                    params.insert(key, value);
                });
            } else {
                view = None;
            }

            let mut is_keep_alive;

            // Keep-Alive is default behavior in HTTP/1.1 and above. Set temporary keep alive
            // behaviour to false if http version is absent or HTTP/1.0
            if let Some(http_version) = request_result.http_version {
                is_keep_alive = http_version != 0;
            } else {
                is_keep_alive = false;
            }

            // Set keep alive true, if the client requests keep alive connection regardless of HTTP version
            if let Some(value) = request_result.headers.value("connection") {
                is_keep_alive = value.to_lowercase() == "keep-alive";
            }

            // Shutdowns next request on the current connection, if the request body is not read
            // completely.

            // Disable extra payload in GET request
            if request_method == "GET" {
                if stream.restored_len().await > 0 {
                    is_keep_alive = false;
                }
            }

            let body_read = Arc::new(AtomicBool::from(true));
            let extra_headers = Arc::new(Mutex::new(Headers::new()));

            let request = Request::from(
                stream.clone(),
                context.clone(),
                scheme.clone(),
                request_method,
                raw_path,
                http_version,
                request_result.headers,
                params,
                query_params,
                session_type.clone(),
                body_read.clone(),
                form_constraints.clone(),
                extra_headers.clone(),
            )
            .await;

            let mut response;
            if let Some(middleware) = middleware {
                racoon_debug!("Middleware found. Passing request to middleware.");
                response = middleware(request, view).await;
            } else {
                response = Path::resolve(request, view).await;
            }

            if !body_read.load(Ordering::Relaxed) {
                racoon_debug!("Request body is not parsed completely. So keep-alive is disabled.");
                is_keep_alive = false;
            }

            // Serves bytes to client
            if response.serve_default() {
                if !is_keep_alive {
                    let headers = response.get_headers();
                    headers.set("Connection", "close");
                }

                let response_bytes = response::response_to_bytes(&mut response);
                let write_result = stream.write_chunk(response_bytes.as_slice()).await;
                if write_result.is_err() {
                    break;
                }
            }

            // Close connection if response explicitly specifies to close or HTTP client does not support
            // keep alive connection.
            if response.should_close() || !is_keep_alive {
                {
                    let _ = stream.shutdown().await;
                }
                break;
            }
        }
    }
}
