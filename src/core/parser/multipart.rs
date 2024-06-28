use std::sync::Arc;

use async_tempfile::TempFile;
use regex::Regex;
use tokio::io::AsyncWriteExt;

use crate::core::headers;
use crate::core::headers::{HeaderValue, Headers};

use crate::core::stream::Stream;

use crate::core::forms::{FileField, Files, FormConstraints, FormData, FormFieldError};

#[derive(Debug)]
pub struct FormPart {
    pub name: Option<String>,
    pub value: Option<String>,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub file: Option<TempFile>,
}

pub struct MultipartParser {
    stream: Arc<Stream>,
    form_constraints: Arc<FormConstraints>,
    boundary: String,
    allow_next_header_read: bool,
    first_header_scanned: bool,
}

impl MultipartParser {
    pub fn from(
        stream: Arc<Stream>,
        headers: &Headers,
        form_constraints: Arc<FormConstraints>,
    ) -> std::io::Result<Self> {
        let content_type;
        if let Some(value) = headers.value("content-type") {
            content_type = value;
        } else {
            return Err(std::io::Error::other("Content-Type header is missing."));
        }

        let boundary = headers::multipart_boundary(&content_type)?;

        Ok(MultipartParser {
            stream,
            form_constraints,
            boundary,
            allow_next_header_read: true,
            first_header_scanned: false,
        })
    }

    pub async fn parse(
        stream: Arc<Stream>,
        form_constraints: Arc<FormConstraints>,
        headers: &Headers,
    ) -> Result<(FormData, Files), FormFieldError> {
        let mut parser = match MultipartParser::from(stream, headers, form_constraints) {
            Ok(parser) => parser,
            Err(error) => {
                return Err(FormFieldError::Others(None, error.to_string(), true));
            }
        };

        let mut form_data = FormData::new();
        let mut files = Files::new();

        loop {
            let mut form_part = parser.next_form_header().await?;
            let parsing_completed = parser.next_form_value(&mut form_part).await?;

            let field_name;
            if let Some(value) = form_part.name {
                field_name = value;
            } else {
                return Err(FormFieldError::Others(
                    None,
                    "Field name is missing.".to_owned(),
                    true,
                ));
            }

            if let Some(filename) = form_part.filename {
                let named_temp_file;
                if let Some(file) = form_part.file {
                    named_temp_file = file;
                } else {
                    return Err(FormFieldError::Others(
                        Some(field_name.clone()),
                        "Parsing error: file is missing.".to_owned(),
                        true,
                    ));
                }

                let temp_file = FileField::from(filename, named_temp_file);
                if let Some(files) = files.get_mut(&field_name) {
                    files.push(temp_file);
                } else {
                    files.insert(field_name, vec![temp_file]);
                }
            } else {
                if let Some(field_value) = form_part.value {
                    if let Some(values) = form_data.get_mut(&field_name) {
                        values.push(field_value);
                    } else {
                        form_data.insert(field_name, vec![field_value]);
                    }
                }
            }

            if parsing_completed {
                return Ok((form_data, files));
            }
        }
    }

    pub async fn next_form_header(&mut self) -> Result<FormPart, FormFieldError> {
        if !self.allow_next_header_read {
            return Err(FormFieldError::Others(
                None,
                "Form part body not read.".to_string(),
                true,
            ));
        }

        let stream = self.stream.clone();
        let max_header_size = self
            .form_constraints
            .max_header_size(stream.buffer_size().await);
        let scan_boundary = format!("--{}\r\n", &self.boundary);
        let scan_boundary_bytes = scan_boundary.as_bytes();

        let mut buffer = vec![];
        let mut bytes_read = 0;

        // Removes starting header for easier pattern matching
        if !self.first_header_scanned {
            // Fetches minimum bytes equal to scan boundary length
            loop {
                if buffer.len() >= scan_boundary.len() {
                    break;
                }

                let chunk = match stream.read_chunk().await {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return Err(FormFieldError::Others(None, error.to_string(), true));
                    }
                };
                bytes_read += chunk.len();
                buffer.extend(chunk);
            }

            if !buffer.starts_with(scan_boundary_bytes) {
                return Err(FormFieldError::Others(
                    None,
                    format!("Boundary does not start with {}", scan_boundary),
                    true,
                ));
            }

            // Removes scan boundary bytes from buffer
            // Contains only form part header
            buffer.drain(0..scan_boundary.len());
            self.first_header_scanned = true;
        }

        const FORM_PART_HEADER_TERMINATOR: &[u8; 4] = b"\r\n\r\n";

        loop {
            if bytes_read > max_header_size {
                return Err(FormFieldError::MaxHeaderSizeExceed);
            }

            let scan_result = buffer
                .windows(FORM_PART_HEADER_TERMINATOR.len())
                .position(|window| window == FORM_PART_HEADER_TERMINATOR);

            if let Some(position) = scan_result {
                let form_part_header_bytes = &buffer[..position];
                let restore_bytes = &buffer[position + FORM_PART_HEADER_TERMINATOR.len()..];
                let _ = stream.restore_payload(restore_bytes.as_ref()).await;

                // Deny next time calling this method because form part body also must be read.
                self.allow_next_header_read = false;
                return Ok(parse_form_part_header(form_part_header_bytes)?);
            } else {
                // Still form part not found. Collect more bytes.
                let chunk = match stream.read_chunk().await {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return Err(FormFieldError::Others(None, error.to_string(), true));
                    }
                };
                bytes_read += chunk.len();
                buffer.extend(chunk);
            }
        }
    }

    pub async fn next_form_value(
        &mut self,
        form_part: &mut FormPart,
    ) -> Result<bool, FormFieldError> {
        if self.allow_next_header_read {
            return Err(FormFieldError::Others(
                None,
                "Form part header is not read.".to_owned(),
                true,
            ));
        }

        if form_part.filename.is_some() {
            Ok(self.parse_file(form_part).await?)
        } else {
            Ok(self.parse_value(form_part).await?)
        }
    }

    pub async fn parse_file(&mut self, form_part: &mut FormPart) -> Result<bool, FormFieldError> {
        let form_constraints = self.form_constraints.clone();
        let field_name;
        if let Some(value) = &form_part.name {
            field_name = value.to_owned();
        } else {
            return Err(FormFieldError::Others(
                None,
                "Field name is missing".to_owned(),
                false,
            ));
        }

        // Form constraints
        let max_file_size =
            form_constraints.max_size_for_file(&field_name, self.stream.buffer_size().await);
        let mut bytes_read = 0;

        let value_terminator = format!("\r\n--{}", self.boundary);
        let value_terminator_bytes = value_terminator.as_bytes();

        let mut temp_file = match TempFile::new().await {
            Ok(file) => match file.open_rw().await {
                Ok(result) => result,
                Err(error) => {
                    return Err(FormFieldError::Others(None, error.to_string(), true));
                }
            },
            Err(error) => {
                return Err(FormFieldError::Others(None, error.to_string(), true));
            }
        };
        let mut scan_buffer = vec![];
        const FORM_PART_END: &[u8; 4] = b"--\r\n";
        const CRLF_BREAK: &[u8; 2] = b"\r\n";

        loop {
            if bytes_read > max_file_size {
                return Err(FormFieldError::MaxFileSizeExceed(field_name.clone()));
            }

            let scan_result = scan_buffer
                .windows(value_terminator_bytes.len())
                .position(|window| window == value_terminator_bytes);

            if let Some(matched_position) = scan_result {
                // File scan reached end
                // Extra offset to check whether file ends or not
                // If extra terminator byte offset is not present, it does not matter whether field
                // end is found or not. Can be scanned again.

                if scan_buffer.len()
                    >= matched_position + value_terminator_bytes.len() + FORM_PART_END.len()
                {
                    let to_copy_position = matched_position;
                    let to_copy = &scan_buffer[..to_copy_position];

                    match temp_file.write_all(to_copy).await {
                        Ok(()) => {}
                        Err(error) => {
                            return Err(FormFieldError::Others(
                                Some(field_name.to_string()),
                                format!("Failed to write file. Error: {}", error),
                                true,
                            ));
                        }
                    }

                    scan_buffer =
                        (&scan_buffer[to_copy_position + value_terminator_bytes.len()..]).to_vec();
                    return if &scan_buffer[..FORM_PART_END.len()] == FORM_PART_END {
                        // Request body completed
                        form_part.file = Some(temp_file);
                        self.allow_next_header_read = true;
                        Ok(true)
                    } else {
                        // Form part completed but body is not ended yet
                        // Skips line break \r\n
                        scan_buffer.drain(..CRLF_BREAK.len());
                        let _ = self.stream.restore_payload(&scan_buffer.as_ref()).await;
                        form_part.file = Some(temp_file);
                        self.allow_next_header_read = true;
                        Ok(false)
                    };
                }
            }

            // Copy data
            if scan_buffer.len() > value_terminator_bytes.len() {
                // This much amount of bytes can be copied safely from the file buffer
                let to_copy_position = scan_buffer.len() - value_terminator_bytes.len();

                match temp_file.write_all(&scan_buffer[..to_copy_position]).await {
                    Ok(()) => {}
                    Err(error) => {
                        return Err(FormFieldError::Others(
                            Some(field_name.to_string()),
                            format!("Failed to write file. Error: {}", error),
                            true,
                        ));
                    }
                }

                scan_buffer.drain(..to_copy_position);
            }

            // File ending has not been reached
            let chunk = match self.stream.read_chunk().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Err(FormFieldError::Others(None, error.to_string(), true));
                }
            };
            bytes_read += chunk.len();
            scan_buffer.extend(chunk);
        }
    }

    pub async fn parse_value(&mut self, form_part: &mut FormPart) -> Result<bool, FormFieldError> {
        let field_name;
        if let Some(value) = &form_part.name {
            field_name = value.to_owned();
        } else {
            return Err(FormFieldError::Others(
                None,
                "Field name is missing.".to_owned(),
                false,
            ));
        }

        let max_value_size = self
            .form_constraints
            .max_size_for_field(&field_name, self.stream.buffer_size().await);
        let scan_boundary = format!("\r\n--{}", self.boundary);
        let scan_boundary_bytes = scan_boundary.as_bytes();

        let mut buffer = vec![];

        const FORM_PART_END: &[u8; 4] = b"--\r\n";
        const CRLF_BREAK: &[u8; 2] = b"\r\n";

        let mut bytes_read = 0;

        loop {
            if bytes_read > max_value_size {
                return Err(FormFieldError::MaxValueSizeExceed(field_name));
            }
            let scan_result = buffer
                .windows(scan_boundary_bytes.len())
                .position(|window| window == scan_boundary_bytes);

            if let Some(position) = scan_result {
                if buffer.len() >= position + scan_boundary_bytes.len() + FORM_PART_END.len() {
                    let to_copy = &buffer[..position];
                    let mut to_copy_range = to_copy.len();

                    // Some clients sends single CRLF and some double CRLF line breaks
                    if to_copy.len() > 1
                        && &to_copy[..to_copy.len() - CRLF_BREAK.len()] == CRLF_BREAK
                    {
                        to_copy_range -= 1;
                    }

                    let value = String::from_utf8_lossy(&to_copy[..to_copy_range]).to_string();

                    // Removes copied bytes from the buffer
                    buffer.drain(..position + scan_boundary_bytes.len());
                    form_part.value = Some(value);

                    return if &buffer[..FORM_PART_END.len()] == FORM_PART_END {
                        self.allow_next_header_read = true;
                        Ok(true)
                    } else {
                        // Form part completed but body is not ended yet
                        // Skips line break \r\n
                        buffer.drain(..CRLF_BREAK.len());
                        let _ = self.stream.restore_payload(buffer.as_ref()).await;
                        self.allow_next_header_read = true;
                        Ok(false)
                    };
                }
            }

            let chunk = match self.stream.read_chunk().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Err(FormFieldError::Others(None, error.to_string(), true));
                }
            };
            bytes_read += chunk.len();
            buffer.extend(chunk);
        }
    }
}

pub fn parse_form_part_header(header_bytes: &[u8]) -> Result<FormPart, FormFieldError> {
    let mut last_scanned_position = 0;
    const HEADER_LINE_TERMINATOR: &[u8; 2] = b"\r\n";

    let mut header_bytes = header_bytes.to_vec();

    // Makes sure scan window reach upto last header line
    if !header_bytes.ends_with(b"\r\n") {
        header_bytes.extend(b"\r\n");
    }

    let mut form_part = FormPart {
        name: None,
        filename: None,
        content_type: None,
        file: None,
        value: None,
    };

    loop {
        let to_scan = &header_bytes[last_scanned_position..];
        let scan_result = to_scan
            .windows(HEADER_LINE_TERMINATOR.len())
            .position(|window| window == HEADER_LINE_TERMINATOR);

        if let Some(relative_position) = scan_result {
            // One header found
            let header_line =
                &header_bytes[last_scanned_position..last_scanned_position + relative_position];
            match parse_form_part_header_line(header_line, &mut form_part) {
                Ok(()) => {}
                Err(error) => {
                    return Err(FormFieldError::Others(None, error.to_string(), true));
                }
            };
            last_scanned_position += relative_position + HEADER_LINE_TERMINATOR.len();
        } else {
            return Ok(form_part);
        }
    }
}

fn parse_form_part_header_line(
    header_line: &[u8],
    form_part: &mut FormPart,
) -> std::io::Result<()> {
    let header_line = String::from_utf8_lossy(header_line);
    let parts: Vec<&str> = header_line.splitn(2, ":").collect();

    if parts.len() != 2 {
        return Ok(());
    }

    let header_name;
    if let Some(name) = parts.get(0) {
        header_name = name.trim();
    } else {
        return Err(std::io::Error::other("Header name is missing."));
    }

    let header_value;
    if let Some(value) = parts.get(1) {
        header_value = *value;
    } else {
        return Err(std::io::Error::other("Header value is missing."));
    }

    if header_name.to_lowercase() == "content-disposition" {
        parse_content_disposition_value(header_value, form_part)?;
    } else if header_name.to_lowercase() == "content-type" {
        form_part.content_type = Some(header_value.trim().to_string());
    }
    Ok(())
}

pub fn parse_content_disposition_value(
    value: &str,
    form_part: &mut FormPart,
) -> std::io::Result<()> {
    let value = value.trim();

    if !value.starts_with("form-data;") {
        // Not a valid Content-Deposition value for form part header
        return Err(std::io::Error::other(
            "Not a valid Content-Deposition value for form part header",
        ));
    }

    let remaining = value.strip_prefix("form-data;").unwrap().trim();
    let pattern = Regex::new(r#"(?<attribute>\w+)="(?<value>[^"]*)""#).unwrap();

    // Goes through all attributes and values
    for captured in pattern.captures_iter(remaining) {
        let attribute = &captured["attribute"];
        let value = &captured["value"];

        if attribute == "name" {
            form_part.name = Some(value.to_string());
        } else if attribute == "filename" {
            form_part.filename = Some(value.to_string());
        }
    }

    if form_part.name.is_none() {
        return Err(std::io::Error::other(
            "Field name is missing in form part header.",
        ));
    }

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use std::{collections::HashMap, sync::Arc};

    use crate::core::forms::{FileFieldShortcut, FormConstraints};
    use crate::core::headers::{HeaderValue, Headers};
    use crate::core::shortcuts::SingleText;
    use crate::core::stream::{AbstractStream, TestStreamWrapper};

    use super::MultipartParser;

    #[tokio::test]
    async fn test_multipart_parser() {
        let mut headers = Headers::new();
        headers.set("Content-Type", "multipart/form-data; boundary=boundary123");

        let test_data = "--boundary123\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nJohn\r\n--boundary123\r\nContent-Disposition: form-data; name=\"location\"\r\n\r\nktm\r\n--boundary123\r\nContent-Disposition: form-data; name=\"file\"; filename=\"example.txt\"\r\nContent-Type: text/plain\r\n\r\nHello World\r\n--boundary123--\r\n".as_bytes().to_vec();
        headers.set("Content-Length", test_data.len().to_string());

        let stream: Box<dyn AbstractStream> = Box::new(TestStreamWrapper::new(test_data, 1024));

        let form_constraints = Arc::new(FormConstraints::new(
            500 * 1024 * 1024,
            2 * 1024 * 1024,
            500 * 1024 * 1024,
            2 * 1024 * 1024,
            HashMap::new(),
        ));

        let parser = MultipartParser::parse(Arc::new(stream), form_constraints, &headers).await;
        assert_eq!(true, parser.is_ok());

        let (form_data, files) = parser.unwrap();
        assert_eq!(Some(&"John".to_string()), form_data.value("name"));
        assert_eq!(Some(&"ktm".to_string()), form_data.value("location"));

        let file_field = files.value("file");
        assert_eq!(true, file_field.is_some());

        let file = file_field.unwrap();
        let file_path = &file.temp_path;
        assert_eq!("example.txt".to_string(), file.name);

        let file_content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!("Hello World".to_string(), file_content);
    }
}
