use std::collections::HashMap;

pub type Headers = HashMap<String, Vec<Vec<u8>>>;

pub trait HeaderValue {
    /// Performs case-insensitive lookup and returns first value.
    fn value<S: AsRef<str>>(&self, name: S) -> Option<String>;

    /// Performs case-insensitive lookup and returns multiple values.
    fn multiple_values<S: AsRef<str>>(&self, name: S) -> Vec<String>;

    /// Inserts new header and makes sure there will be only one header with the given name.
    fn set<B: AsRef<[u8]>>(&mut self, name: &str, value: B);
    
    /// Inserts new headers and allows to have multiple headers with the same name.
    fn set_multiple<B: AsRef<[u8]>>(&mut self, name: &str, value: B);
}

impl HeaderValue for Headers {
    fn value<S: AsRef<str>>(&self, name: S) -> Option<String> {
        let name = name.as_ref();

        for (key, values) in self.iter() {
            if key.to_lowercase() != name.to_lowercase() {
                continue;
            }

            if let Some(value_bytes) = values.get(0) {
                let value = String::from_utf8_lossy(value_bytes);
                return Some(value.to_string());
            }
        }

        None
    }

    fn multiple_values<S: AsRef<str>>(&self, name: S) -> Vec<String> {
        let name = name.as_ref();

        let mut multiple_headers = vec![];

        for (key, values) in self.iter() {
            if key.to_lowercase() != name.to_lowercase() {
                continue;
            }

            if let Some(value_bytes) = values.get(0) {
                let value = String::from_utf8_lossy(value_bytes);
                multiple_headers.push(value.to_string());
            }
        }

        multiple_headers
    }

    fn set<B: AsRef<[u8]>>(&mut self, name: &str, value: B) {
        let value = value.as_ref();

        if let Some(values) = self.get_mut(&name.to_string()) {
            if values.len() > 0 {
                values.clear();
            }

            values.push(value.to_vec());
        } else {
            self.insert(name.to_string(), vec![value.to_vec()]);
        };
    }

    fn set_multiple<B: AsRef<[u8]>>(&mut self, name: &str, value: B) {
        let value = value.as_ref();

        if let Some(values) = self.get_mut(&name.to_string()) {
            values.push(value.to_vec());
        } else {
            self.insert(name.to_string(), vec![value.to_vec()]);
        };
    }
}

///
/// # Example
///
/// ```
/// use racoon::core::headers::multipart_boundary;
///
/// let boundary_string = "application/form_data; boundary=----123456";
///
/// assert_eq!(multipart_boundary(&boundary_string.to_string()).is_ok(), true);
/// assert_eq!(multipart_boundary(&boundary_string.to_string()).unwrap(), "----123456");
///
/// ```
///
pub fn multipart_boundary(content_type: &String) -> std::io::Result<String> {
    let value: Vec<&str> = content_type.split(";").collect();

    if value.len() >= 2 {
        let content_type_text = value.get(1).unwrap().trim();
        let boundary = content_type_text.strip_prefix("boundary=").unwrap();
        return Ok(boundary.to_string());
    }

    return Err(std::io::Error::other("Boundary missing."));
}

#[cfg(test)]
pub mod tests {
    use crate::core::headers::{multipart_boundary, HeaderValue, Headers};

    #[test]
    pub fn test_header_value() {
        let mut headers = Headers::new();
        headers.set("Content-Type", b"text/html");

        // Case-insensitive
        assert_eq!(headers.value("content-Type").is_some(), true);
        assert_eq!(
            headers.value("content-Type").unwrap(),
            "text/html".to_string()
        );

        // Case sensitive
        assert_eq!(headers.get("content-Type").is_some(), false);
    }

    #[test]
    pub fn test_multipart_boundary() {
        let boundary_string = "application/form_data; boundary=----123456";

        assert_eq!(
            multipart_boundary(&boundary_string.to_string()).is_ok(),
            true
        );
        assert_eq!(
            multipart_boundary(&boundary_string.to_string()).unwrap(),
            "----123456"
        );
    }
}
