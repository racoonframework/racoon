pub mod managers;

use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::core::headers::Headers;

pub type SessionResult<T> = Box<dyn Future<Output = T> + Send + Unpin>;

pub trait AbstractSessionManager: Sync + Send {
    /// Set or update session value of the client.
    fn set(
        &self,
        session_id: &String,
        name: &str,
        value: &str,
    ) -> SessionResult<std::io::Result<()>>;

    /// Returns session value of the client.
    fn get(&self, session_id: &String, name: &str) -> SessionResult<Option<String>>;

    /// Removes session key and value of the client.
    fn remove(&self, session_id: &String, name: &str) -> SessionResult<std::io::Result<()>>;

    /// Removes all session key and value of the client.
    fn destroy(&self, session_id: &String) -> SessionResult<std::io::Result<()>>;
}

pub type SessionManager = Box<dyn AbstractSessionManager>;

pub struct Session {
    session_manager: Arc<SessionManager>,
    session_id: String,
    response_headers: Arc<Mutex<Headers>>,
}

impl Clone for Session {
    fn clone(&self) -> Self {
        Self {
            session_manager: self.session_manager.clone(),
            session_id: self.session_id.clone(),
            response_headers: self.response_headers.clone(),
        }
    }
}

impl Session {
    pub fn from(
        session_manager: Arc<SessionManager>,
        session_id: Option<&String>,
        response_headers: Arc<Mutex<Headers>>,
    ) -> Self {
        let session_id_value;

        // If not exists, creates new session id
        if let Some(session_id) = session_id {
            session_id_value = session_id.to_owned();
        } else {
            session_id_value = Uuid::new_v4().to_string();
        }

        Self {
            session_manager,
            session_id: session_id_value,
            response_headers: response_headers.clone(),
        }
    }

    ///
    /// Session id of the client received from the cookie header `sessionid`. The request instance automatically initializes
    /// with new value if the `sessionid` header is not present.
    ///
    pub async fn session_id(&self) -> &String {
        &self.session_id
    }

    ///
    /// Set or update exisiting session value.
    ///
    /// # Examples
    /// ```
    /// use racoon::core::request::Request;
    ///
    /// async fn home(request: Request) {
    ///   let session = request.session;
    ///   let _ = session.set("name", "John").await;
    /// }
    /// ```
    ///
    pub async fn set<S: AsRef<str>>(&self, name: S, value: S) -> std::io::Result<()> {
        match self
            .session_manager
            .set(&self.session_id, name.as_ref(), value.as_ref())
            .await
        {
            Ok(()) => return Ok(()),
            Err(error) => {
                return Err(std::io::Error::other(error));
            }
        };
    }

    ///
    /// Returns session value of type `Option<String>`.
    ///
    /// # Examples
    /// ```
    /// use racoon::core::request::Request;
    ///
    /// async fn home(request: Request) {
    ///   let session = request.session;
    ///   let name = session.get("name").await;
    /// }
    /// ```
    ///
    /// This method does not return or print any error message by default.
    /// ```
    /// use racoon::core::server::Server;
    ///
    /// // Enable debugging
    /// Server::enable_logging();
    /// ```
    ///
    pub async fn get<S: AsRef<str>>(&self, name: S) -> Option<String> {
        self.session_manager
            .get(&self.session_id, name.as_ref())
            .await
    }

    ///
    /// Removes session value.
    ///
    /// # Examples
    /// ```
    /// use racoon::core::request::Request;
    ///
    /// async fn home(request: Request) {
    ///   let session = request.session;
    ///   let _ = session.remove("name").await;
    /// }
    /// ```
    ///
    pub async fn remove<S: AsRef<str>>(&self, name: S) -> std::io::Result<()> {
        self.session_manager
            .remove(&self.session_id, name.as_ref())
            .await
    }

    ///
    /// Removes all session values of the client.
    ///
    pub async fn destroy(&self) -> std::io::Result<()> {
        // Removes sesisonid from Cookie
        let response_headers_ref = self.response_headers.clone();
        let mut response_headers = response_headers_ref.lock().await;

        let expire_header_value = format!(
            "{}=;Expires=Sun, 06 Nov 1994 08:49:37 GMT; Path=/",
            "sessionid"
        );
        response_headers.insert("Set-Cookie".to_string(), vec![expire_header_value.as_bytes().to_vec()]);

        self.session_manager.destroy(&self.session_id).await
    }
}
