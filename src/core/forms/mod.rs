use std::collections::HashMap;

use async_tempfile::TempFile;

#[derive(Debug)]
pub struct FileField {
    pub name: String,
    pub temp_file: TempFile,
}

pub type Files = HashMap<String, Vec<FileField>>;
pub type FormData = HashMap<String, Vec<String>>;

pub trait FileFieldShortcut {
    /// Performs case-insensitive lookup and returns first file found.
    fn value<S: AsRef<str>>(&self, name: S) -> Option<&FileField>;
}


impl FileFieldShortcut for Files {
    fn value<S: AsRef<str>>(&self, name: S) -> Option<&FileField> {
        let name = name.as_ref();

        for (key, values) in self.iter() {
            if key.to_lowercase() != name.to_lowercase() {
                continue;
            }

            if let Some(field) = values.get(0) {
                return Some(field);
            }
        }
        None
    }
}

///
/// The form constraint works as a security measure while parsing request body.
/// It can be set globally while creating the `Server` instance.
///
/// # Example
///
/// ```markdown
///
/// Server::bind("127.0.0.1:8080")
///  .urls(paths)
///  .form_constraints(FormConstraints {...})
///  .run().await;
/// ```
///
pub struct FormConstraints {
    /// Maximum allowed body size.
    max_body_size: usize,
    /// Maximum allowed form part header size.
    max_header_size: usize,
    /// Maximum allowed form part file size.
    max_file_size: usize,
    /// Maximum allowed form field value size.
    max_value_size: usize,
    /// Map of field name and maximum allowed size.
    custom_max_sizes: HashMap<String, usize>,
}

impl FormConstraints {
    pub fn new(max_body_size: usize, max_header_size: usize, max_file_size: usize,
               max_value_size: usize, custom_max_sizes: HashMap<String, usize>) -> Self {
        Self {
            max_body_size,
            max_header_size,
            max_file_size,
            max_value_size,
            custom_max_sizes,
        }
    }

    pub fn max_body_size(&self, buffer_size: usize) -> usize {
        if buffer_size > self.max_body_size {
            return buffer_size;
        }

        // Default size
        self.max_body_size
    }

    pub fn max_header_size(&self, buffer_size: usize) -> usize {
        if buffer_size > self.max_header_size {
            return buffer_size;
        }

        // Default size
        self.max_header_size
    }

    pub fn max_value_size(&self, buffer_size: usize) -> usize {
        if buffer_size > self.max_value_size {
            return buffer_size;
        }

        // Default size
        self.max_value_size
    }
    pub fn max_size_for_field(&self, field_name: &String, buffer_size: usize) -> usize {
        if let Some(max_size) = self.custom_max_sizes.get(field_name) {
            if buffer_size > *max_size {
                return buffer_size;
            }
            return max_size.to_owned();
        }

        // Default size
        return self.max_value_size;
    }

    pub fn max_size_for_file(&self, field_name: &String, buffer_size: usize) -> usize {
        if let Some(max_size) = self.custom_max_sizes.get(field_name) {
            if buffer_size > *max_size {
                return buffer_size;
            }
            return max_size.to_owned();
        }

        // Default size
        return self.max_file_size;
    }
}

#[derive(Debug)]
pub enum FormFieldError {
    /// Max form part body size exceeded.
    MaxBodySizeExceed,
    /// Maximum form part header size exceeded.
    MaxHeaderSizeExceed,
    /// Maximum file size exceeded.
    MaxFileSizeExceed(String),
    /// Maximum length of text length exceeded.
    MaxValueSizeExceed(String),
    /// (field_name, error, is_criticial)
    /// If error is critical, don't expose to client.
    Others(Option<String>, String, bool),
}
