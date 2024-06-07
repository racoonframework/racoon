use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};

use crate::core::headers::{HeaderValue, Headers};
use crate::core::shortcuts::SingleText;

pub type Cookies = HashMap<String, String>;

impl SingleText for Cookies {
    fn value<S: AsRef<str>>(&self, name: S) -> Option<&String> {
        let name = name.as_ref();

        for cookie_name in self.keys() {
            if cookie_name.to_lowercase() != name.to_lowercase() {
                continue;
            }

            let value = self.get(cookie_name);
            return value;
        }

        None
    }
}

///
/// Returns HashMap of type Cookies from passed headers.
///
pub fn parse_cookies_from_header(headers: &Headers) -> Cookies {
    // Reads Cookie header value from multiple header lines.
    // Example:
    // Cookie: name=John;
    // Cookie: location=ktm;
    let cookie_headers = headers.multiple_values("cookie");
    let mut cookies = Cookies::new();

    // Looping through multiple "Cookie: ..." headers.
    for cookie_header_value in cookie_headers {
        parse_cookie_header_value(cookie_header_value, &mut cookies);
    }

    cookies
}

///
/// # Example
///
/// ```
/// use racoon::core::cookie::parse_cookie_header_value;
/// use racoon::core::cookie::Cookies;
/// use racoon::core::shortcuts::SingleText;
///
/// // Requires only value from "Cookie: name=John; location=Ktm"
/// let cookie_header_value = "name=John; location=Ktm".to_string();
/// let mut cookies = Cookies::new();
///
/// parse_cookie_header_value(cookie_header_value, &mut cookies);
///
/// let name = cookies.value("name");
/// let location = cookies.value("location");
/// let unknown = cookies.value("unknown");
///
/// assert_eq!(name, Some(&"John".to_string()));
/// assert_eq!(location, Some(&"Ktm".to_string()));
/// assert_eq!(unknown, None);
/// ```
///
pub fn parse_cookie_header_value(cookie_header_value: String, cookies: &mut Cookies) {
    // Single Cookie header value contains multiple key value pairs seperated by comma.
    let raw_key_values: Vec<&str> = cookie_header_value.split(";").collect();

    for raw_value in raw_key_values {
        let key_value: Vec<&str> = (*raw_value).splitn(2, "=").collect();

        if key_value.len() >= 2 {
            let raw_key = key_value[0].trim();
            // If url decoding fails, raw values are used.
            let key = match urlencoding::decode(raw_key) {
                Ok(decoded) => decoded.to_string(),
                Err(_) => raw_key.to_string(),
            };

            let raw_value = key_value[1].trim();
            let value = match urlencoding::decode(raw_value) {
                Ok(decoded) => decoded.to_string(),
                Err(_) => raw_value.to_string(),
            };

            cookies.insert(key, value);
        }
    }
}

pub fn set_cookie<S: AsRef<str>>(headers: &mut Headers, name: S, value: S, max_age: Duration) {
    let now = SystemTime::now();
    let expire_time = now + max_age;
    let datetime = DateTime::<Utc>::from(expire_time);
    let expires_date = datetime.format("%a, %d-%b-%Y %H:%M:%S GMT");

    let encoded_name = urlencoding::encode(name.as_ref());
    let encoded_value = urlencoding::encode(value.as_ref());

    let header_value = format!(
        "{}={}; Expires={}; Path=/; HttpOnly",
        encoded_name, encoded_value, expires_date
    );
    headers.set_multiple("Set-Cookie", header_value);
}

