use std::collections::HashMap;
use std::sync::Arc;

use crate::core::forms::{FormConstraints, FormData, FormFieldError};
use crate::core::headers::{HeaderValue, Headers};
use crate::core::parser::params::parse_url_encoded;

use crate::core::stream::Stream;

pub type FormFields = HashMap<String, Vec<String>>;

pub struct UrlEncodedParser {
    stream: Arc<Stream>,
    form_constraints: Arc<FormConstraints>,
    content_length: usize,
}

impl UrlEncodedParser {
    pub fn from(
        stream: Arc<Stream>,
        headers: &Headers,
        form_constraints: Arc<FormConstraints>,
    ) -> Result<UrlEncodedParser, FormFieldError> {
        let content_length;
        if let Some(value) = headers.value("Content-Length") {
            content_length = match value.parse::<usize>() {
                Ok(value) => value,
                Err(_) => {
                    return Err(FormFieldError::Others(
                        None,
                        "Invalid content length header.".to_owned(),
                    ));
                }
            }
        } else {
            return Err(FormFieldError::Others(
                None,
                "Content-Length header is missing.".to_owned(),
            ));
        }

        Ok(UrlEncodedParser {
            stream,
            form_constraints,
            content_length,
        })
    }

    ///
    /// Reads body from the stream equal to the `Content-Length` specified in the header, decodes
    /// url encoded raw body and returns the result.
    ///
    async fn read_query_params_from_stream(&self) -> Result<FormFields, FormFieldError> {
        let max_body_size = self
            .form_constraints
            .max_body_size(self.stream.buffer_size().await);

        if self.content_length > max_body_size {
            return Err(FormFieldError::MaxBodySizeExceed);
        }

        let mut buffer = vec![];

        loop {
            if buffer.len() >= self.content_length {
                let value = String::from_utf8_lossy(&buffer);
                return Ok(parse_url_encoded(value.to_string().as_str()));
            }

            let chunk = match self.stream.read_chunk().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Err(FormFieldError::Others(None, error.to_string()));
                }
            };
            buffer.extend(chunk);
        }
    }

    ///
    /// Returns parsing result for url encoded request body considering form constraints.
    ///
    pub async fn parse(
        stream: Arc<Stream>,
        headers: &Headers,
        form_constraints: Arc<FormConstraints>,
    ) -> Result<FormData, FormFieldError> {
        let parser = UrlEncodedParser::from(stream, headers, form_constraints)?;
        let params = parser.read_query_params_from_stream().await?;
        Ok(params)
    }
}

#[cfg(test)]
pub mod test {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::core::forms::{FormConstraints, FormFieldError};
    use crate::core::headers::{HeaderValue, Headers};
    use crate::core::shortcuts::SingleText;
    use crate::core::stream::{AbstractStream, TestStreamWrapper};

    use super::UrlEncodedParser;

    #[tokio::test()]
    async fn test_url_encode_parser() {
        let mut headers = Headers::new();
        let test_data = b"name=John&location=ktm".to_vec();
        headers.set("Content-Length", test_data.len().to_string());

        let stream: Box<dyn AbstractStream> = Box::new(TestStreamWrapper::new(test_data, 1024));

        let form_constraints = Arc::new(FormConstraints::new(
            2 * 1024 * 1024,
            2 * 1024 * 1024,
            500 * 1024 * 1024,
            2 * 1024 * 1024,
            HashMap::new(),
        ));

        let url_encode_parser =
            UrlEncodedParser::parse(Arc::new(stream), &headers, form_constraints).await;
        assert_eq!(true, url_encode_parser.is_ok());

        let parse_result = url_encode_parser.unwrap();
        assert_eq!(Some(&"John".to_string()), parse_result.value("name"));
        assert_eq!(Some(&"ktm".to_string()), parse_result.value("location"));
    }

    #[tokio::test()]
    async fn test_no_content_length_parsing() {
        let headers = Headers::new();
        let test_data = b"name=John&location=ktm".to_vec();

        let stream: Box<dyn AbstractStream> = Box::new(TestStreamWrapper::new(test_data, 1024));

        let form_constraints = Arc::new(FormConstraints::new(
            2 * 1024 * 1024,
            2 * 1024 * 1024,
            500 * 1024 * 1024,
            2 * 1024 * 1024,
            HashMap::new(),
        ));

        let url_encode_parser =
            UrlEncodedParser::parse(Arc::new(stream), &headers, form_constraints).await;
        assert_eq!(true, url_encode_parser.is_err());

        let form_field_error = url_encode_parser.unwrap_err();
        match form_field_error {
            FormFieldError::Others(_, _) => {
            }
            _ => {
                assert!(true)
            }
        }
    }
}
