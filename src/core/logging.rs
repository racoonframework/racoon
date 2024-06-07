pub mod condition {
    use std::env;

    pub fn is_logging_enabled() -> bool {
        return match env::var("RACOON_LOGGING") {
            Ok(value) => {
                value.to_lowercase() == "true"
            }
            Err(_) => {
                false
            }
        };
    }
}

#[macro_export]
macro_rules! racoon_debug {
    ($($arg:tt)*) => {
        if crate::core::logging::condition::is_logging_enabled() {
            log::debug!($($arg)*);
        }
    }
}

#[macro_export]
macro_rules! racoon_info {
    ($($arg:tt)*) => {
        if crate::core::logging::condition::is_logging_enabled() {
            log::info!($($arg)*);
        }
    }
}

#[macro_export]
macro_rules! racoon_warn {
    ($($arg:tt)*) => {
        if use crate::core::logging::condition::is_logging_enabled() {
            log::warn!($($arg)*);
        }
    }
}

#[macro_export]
macro_rules! racoon_trace {
    ($($arg:tt)*) => {
        if use crate::core::logging::condition::is_logging_enabled() {
            log::trace!($($arg)*);
        }
    }
}

#[macro_export]
macro_rules! racoon_error {
    ($($arg:tt)*) => {
        if crate::core::logging::condition::is_logging_enabled() {
            log::error!($($arg)*);
        }
    }
}
