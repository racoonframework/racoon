use std::collections::HashMap;
use std::sync::Arc;

use crate::core::forms::{
    FormConstraints,
    FormData,
    FormFieldError
};
use crate::core::headers::{Headers, HeaderValue};
use crate::core::parser::params::parse_url_encoded;

use crate::core::stream::Stream;

pub type FormFields = HashMap<String, Vec<String>>;

pub struct UrlEncodedParser {
    stream: Arc<Stream>,
    form_constraints: Arc<FormConstraints>,
    content_length: usize,
}

impl UrlEncodedParser {
    pub fn from(stream: Arc<Stream>, headers: &Headers,
                form_constraints: Arc<FormConstraints>) -> Result<UrlEncodedParser, FormFieldError> {
        let content_length;
        if let Some(value) = headers.value("Content-Length") {
            content_length = match value.parse::<usize>() {
                Ok(value) => value,
                Err(_) => {
                    return Err(FormFieldError::Others("Invalid content length header.".to_owned()));
                }
            }
        } else {
            return Err(FormFieldError::Others("Content-Length header is missing.".to_owned()));
        }

        Ok(UrlEncodedParser {
            stream,
            form_constraints,
            content_length,
        })
    }

    pub async fn query_params(&self) -> Result<FormFields, FormFieldError> {
        let max_body_size = self.form_constraints.max_body_size(self.stream.buffer_size().await);

        if self.content_length > max_body_size {
            return Err(FormFieldError::MaxValueSizeExceed);
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
                    return Err(FormFieldError::Others(error.to_string()));
                }
            };
            buffer.extend(chunk);
        }
    }

    pub async fn parse(stream: Arc<Stream>, headers: &Headers, form_constraints: Arc<FormConstraints>)
                       -> Result<FormData, FormFieldError> {
        let parser = UrlEncodedParser::from(stream, headers, form_constraints)?;
        let params = parser.query_params().await?;
        Ok(params)
    }
}
