use std::env;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::ConnectOptions;
use sqlx::Executor;
use sqlx::Pool;
use sqlx::Sqlite;

use crate::core::session::AbstractSessionManager;
use crate::core::session::SessionResult;
use crate::racoon_debug;
use crate::racoon_error;

///
/// FileSessionManager is a default session manager based on the Sqlite database. The database is stored on
/// `.cache/session` file.
///
/// # Examples
///
/// ```
/// use std::env;
///
/// use racoon::core::session::managers::FileSessionManager;
///
/// #[tokio::main]
/// async fn main() {
///   // Optional
///   env::set_var("SESSION_FILE_PATH", "../mydb/session");
///   let session_manager = FileSessionManager::new().await;
/// }
/// ```
///
/// The file path can be specified by specifying `SESSION_FILE_PATH` in environment variable.
///
pub struct FileSessionManager {
    db_connection: Arc<Option<Pool<Sqlite>>>,
}

impl FileSessionManager {
    ///
    /// Creates new instance of FileSessonManager.
    ///
    pub async fn new() -> std::io::Result<Self> {
        let instance = Self {
            db_connection: Arc::new(None),
        };
        Ok(instance)
    }

    ///
    /// Returns stored session file path.
    ///
    /// If environment variable `SESSION_FILE_PATH` is specified, it will return the specified path
    /// else default relative file path `.cache/session`.
    ///
    fn get_db_path() -> String {
        env::var("SESSION_FILE_PATH").unwrap_or(".cache/session".to_string())
    }

    ///
    /// Returns Sqlite pool lazily. If connection pool is not already initialized, it initializes
    /// new Sqlite database, creates table and returns the new initialized connection pool.
    ///
    async fn lazy_connection_pool(
        mut db_connection: Arc<Option<Pool<Sqlite>>>,
    ) -> std::io::Result<Pool<Sqlite>> {
        if let Some(db_pool) = db_connection.as_ref() {
            return Ok(db_pool.clone());
        }

        let db_path = PathBuf::from(FileSessionManager::get_db_path());
        let db_exists;

        if !db_path.exists() {
            racoon_debug!("Session database does not exist. Creating new one.");

            // Session database directory
            let mut db_dir = db_path.clone();
            db_dir.pop();

            db_exists = false;
            std::fs::create_dir_all(db_dir)?;
            std::fs::File::create_new(&db_path)?;
        } else {
            db_exists = true;
        }

        // Disables sqlx logging
        let connect_options =
            match SqliteConnectOptions::from_str(db_path.to_string_lossy().as_ref()) {
                Ok(options) => options.disable_statement_logging(),
                Err(error) => {
                    return Err(std::io::Error::other(format!(
                        "Failed to create sqlite connect options for session database. Error: {}",
                        error
                    )));
                }
            };

        match sqlx::SqlitePool::connect_with(connect_options).await {
            Ok(pool) => {
                if !db_exists {
                    const CREATE_SESSION_TABLE_QUERY: &str = r#"
                        CREATE TABLE session(
                            id BIGINT AUTO_INCREMENT PRIMARY KEY, 
                            session_id VARCHAR(1025) NOT NULL,
                            key TEXT NOT NULL UNIQUE,
                            value TEXT NOT NULL
                        )
                    "#;

                    match pool.execute(CREATE_SESSION_TABLE_QUERY).await {
                        Ok(_) => {
                            racoon_debug!("Created session table.");
                        }
                        Err(error) => {
                            return Err(std::io::Error::other(format!(
                                "Failed to create session table. Error: {}",
                                error
                            )));
                        }
                    };
                }
                db_connection = Arc::from(Some(pool.clone()));

                if let Some(db_connection) = db_connection.as_ref() {
                    return Ok(db_connection.clone());
                }

                return Err(std::io::Error::other("Error reading connection pool."));
            }
            Err(error) => {
                return Err(std::io::Error::other(format!(
                    "Failed to connect sqlite db for managing session. Error: {:?}",
                    error
                )));
            }
        }
    }
}

impl AbstractSessionManager for FileSessionManager {
    fn set(
        &self,
        session_id: &String,
        name: &str,
        value: &str,
    ) -> SessionResult<std::io::Result<()>> {
        let db_connection = self.db_connection.clone();
        let session_id = session_id.to_owned();
        let key = name.to_string();
        let value = value.to_string();

        Box::new(Box::pin(async move {
            let db_pool = match Self::lazy_connection_pool(db_connection.clone()).await {
                Ok(pool) => pool,
                Err(error) => {
                    return Err(error);
                }
            };

            const UPSERT_QUERY: &str = r#"
                INSERT INTO session(session_id, key, value) 
                VALUES ($1, $2, $3)
                ON CONFLICT(key) DO UPDATE 
                SET 
                    session_id=excluded.session_id, 
                    key=excluded.key,
                    value=excluded.value
            "#;

            let result = sqlx::query(UPSERT_QUERY)
                .bind(session_id)
                .bind(key)
                .bind(value)
                .execute(&db_pool)
                .await;

            match result {
                Ok(_) => {}
                Err(error) => {
                    return Err(std::io::Error::other(format!(
                        "Failed to set session value. Error: {}",
                        error
                    )));
                }
            };

            Ok(())
        }))
    }

    fn get(&self, session_id: &String, name: &str) -> SessionResult<Option<String>> {
        let db_connection = self.db_connection.clone();
        let session_id = session_id.to_owned();
        let key = name.to_string();

        Box::new(Box::pin(async move {
            let db_pool = match Self::lazy_connection_pool(db_connection.clone()).await {
                Ok(pool) => pool,
                Err(error) => {
                    racoon_error!(
                        "Failed to create session database connection pool. Error: {}",
                        error
                    );
                    return None;
                }
            };

            const FETCH_QUERY: &str = r#"
                SELECT value FROM session 
                WHERE 
                    session_id=$1 AND key=$2 
                LIMIT 1
            "#;

            let result: Result<(String,), sqlx::Error> = sqlx::query_as(FETCH_QUERY)
                .bind(session_id)
                .bind(key)
                .fetch_one(&db_pool)
                .await;

            return match result {
                Ok((value,)) => Some(value),
                Err(error) => {
                    racoon_debug!("Failed to fetch session value. Error: {}", error);
                    return None;
                }
            };
        }))
    }

    fn remove(&self, session_id: &String, name: &str) -> SessionResult<std::io::Result<()>> {
        let db_connection = self.db_connection.clone();
        let session_id = session_id.to_owned();
        let key = name.to_string();

        Box::new(Box::pin(async move {
            let db_pool = match Self::lazy_connection_pool(db_connection.clone()).await {
                Ok(pool) => pool,
                Err(error) => {
                    return Err(std::io::Error::other(format!(
                        "Failed to create session database connection pool. Error: {}",
                        error
                    )));
                }
            };

            const DELETE_QUERY: &str = r#"
                DELETE * FROM session WHERE session_id=$1 AND key=$2
            "#;

            let result = sqlx::query(DELETE_QUERY)
                .bind(session_id)
                .bind(key)
                .execute(&db_pool)
                .await;

            return match result {
                Ok(_) => Ok(()),
                Err(error) => Err(std::io::Error::other(format!(
                    "Failed to delete session values. Error: {}",
                    error
                ))),
            };
        }))
    }

    fn destroy(&self, session_id: &String) -> SessionResult<std::io::Result<()>> {
        let db_connection = self.db_connection.clone();
        let session_id = session_id.to_owned();

        Box::new(Box::pin(async move {
            let db_pool = match Self::lazy_connection_pool(db_connection.clone()).await {
                Ok(pool) => pool,
                Err(error) => {
                    return Err(std::io::Error::other(format!(
                        "Failed to create session database connection pool. Error: {}",
                        error
                    )));
                }
            };

            const DELETE_QUERY: &str = r#"
                DELETE * FROM session WHERE session_id=$1
                "#;

            let result = sqlx::query(DELETE_QUERY)
                .bind(session_id)
                .execute(&db_pool)
                .await;

            return match result {
                Ok(_) => Ok(()),
                Err(error) => Err(std::io::Error::other(format!(
                    "Failed to delete all session values. Error: {}",
                    error
                ))),
            };
        }))
    }
}
