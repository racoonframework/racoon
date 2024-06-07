pub mod multipart;
pub mod urlencoded;

pub mod headers {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::core::headers::{Headers, HeaderValue};
    use crate::core::request::RequestError;
    use crate::core::server::RequestConstraints;
    use crate::core::stream::Stream;

    #[derive(Debug)]
    pub struct RequestHeaderResult {
        pub method: Option<String>,
        pub http_version: Option<u8>,
        pub raw_path: Option<String>,
        pub headers: Headers,
    }

    impl RequestHeaderResult {
        pub fn new() -> Self {
            Self {
                method: None,
                http_version: None,
                raw_path: None,
                headers: HashMap::new(),
            }
        }
    }

    pub async fn read_request_headers(stream: Arc<Stream>,
                                      request_constraints: Arc<RequestConstraints>)
                                      -> Result<RequestHeaderResult, RequestError> {
        let max_request_header_size = request_constraints.max_request_header_size(stream.buffer_size().await);

        let mut buffer: Vec<u8> = vec![];

        let mut bytes_read = 0;

        loop {
            let chunk = match stream.read_chunk().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Err(RequestError::Others(error.to_string()));
                }
            };
            bytes_read += chunk.len();
            buffer.extend(chunk);

            if bytes_read > max_request_header_size {
                return Err(RequestError::HeaderSizeExceed);
            }

            let mut headers = vec![httparse::EMPTY_HEADER; request_constraints.max_header_count];
            let mut request = httparse::Request::new(&mut headers);
            let result = request.parse(&buffer);

            match result {
                Ok(status) => {
                    if status.is_partial() {
                        continue;
                    }

                    let matched_position = status.unwrap();
                    let partial_body = &buffer[matched_position..];
                    let _ = stream.restore_payload(partial_body).await;

                    let request_method;
                    if let Some(method) = request.method {
                        request_method = Some(method.to_string());
                    } else {
                        request_method = None;
                    }

                    let http_version;
                    if let Some(version) = request.version {
                        http_version = Some(version);
                    } else {
                        http_version = None;
                    }

                    let path;
                    if let Some(request_path) = request.path {
                        path = Some(request_path.to_owned());
                    } else {
                        path = None;
                    }

                    let mut headers = HashMap::new();
                    request.headers.iter().for_each(|header| {
                        headers.set_multiple(header.name, header.value);
                    });

                    if status.is_complete() {
                        return Ok(RequestHeaderResult {
                            method: request_method,
                            http_version,
                            raw_path: path,
                            headers,
                        });
                    }
                }
                Err(_) => {
                    // Not actual error
                    // Wait until header is not completely found
                }
            }
        }
    }
}


pub mod path {
    ///
    /// Does not include `?` character in raw query.
    ///
    pub fn path_and_raw_query<S: AsRef<str>>(raw_path: S) -> (String, String) {
        let raw_path = raw_path.as_ref().to_string();
        let split: Vec<&str> = raw_path.splitn(2, "?").collect();

        let path;
        if let Some(value) = split.get(0) {
            path = value.to_string();
        } else {
            path = raw_path.to_owned();
        }

        let raw_query;
        if let Some(value) = split.get(1) {
            raw_query = value.to_string();
        } else {
            raw_query = "".to_owned();
        }

        return (path, raw_query);
    }
}

pub mod params {
    use std::collections::HashMap;
    use crate::core::parser::path::path_and_raw_query;

    ///
    /// # Examples
    /// ```
    /// use racoon::core::shortcuts::SingleText;
    /// use racoon::core::parser::params::query_params_from_raw;
    ///
    /// let raw_path = "?name=John&location=ktm";
    /// let query_params = query_params_from_raw(raw_path);
    ///
    /// let name = query_params.value("name");
    /// let location = query_params.value("location");
    /// let unknown = query_params.value("unknown");
    /// assert_eq!(name, Some(&"John".to_string()));
    /// assert_eq!(location, Some(&"ktm".to_string()));
    /// assert_eq!(unknown, None);
    ///
    /// ```
    ///
    pub fn query_params_from_raw<S: AsRef<str>>(raw_path: S) -> HashMap<String, Vec<String>> {
        let (_, raw_query) = path_and_raw_query(raw_path.as_ref());
        parse_url_encoded(&raw_query)
    }

    pub fn parse_url_encoded<S: AsRef<str>>(text: S) -> HashMap<String, Vec<String>> {
        let text = text.as_ref();
        let mut params = HashMap::new();
        if text.len() == 0 {
            return params;
        }

        let values = text.split("&");

        for value in values {
            let key_values: Vec<&str> = value.split("=").collect();
            if key_values.len() >= 2 {
                let name = key_values.get(0).unwrap();
                let value = key_values.get(1).unwrap();

                let name_formatted = match urlencoding::decode(name) {
                    Ok(value) => value.to_string(),
                    Err(_) => name.to_string()
                };

                let value_formatted = match urlencoding::decode(value) {
                    Ok(value) => value.to_string(),
                    Err(_) => value.to_string()
                };

                if !params.contains_key(&name_formatted) {
                    params.insert(name.to_string(), Vec::new());
                }

                let values = params.get_mut(&name_formatted).unwrap();
                values.push(value_formatted);
            }
        }
        return params;
    }
}
