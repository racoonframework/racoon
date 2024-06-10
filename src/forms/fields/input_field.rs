use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::forms::{Files, FormData};

use crate::forms::fields::FieldResult;
use crate::forms::AbstractFields;

pub enum InputFieldError<'a> {
    MissingField(&'a String),
    /// (field_name, value, minimum_length)
    MinimumLengthRequired(&'a String, &'a String, &'a usize),
    /// (field_name, value, maximum_length)
    MaximumLengthExceed(&'a String, &'a String, &'a usize),
}

#[derive(Clone)]
pub struct InputField {
    field_name: String,
    max_length: Arc<usize>,
    required: Arc<AtomicBool>,
    value: Arc<Mutex<Option<String>>>,
    error_handler: Option<Arc<Box<fn(InputFieldError, Vec<String>) -> Vec<String>>>>,
    default_value: Option<String>,
}

impl InputField {
    pub fn with<S: AsRef<str>>(field_name: S, max_length: usize) -> Self {
        let field_name = field_name.as_ref().to_string();

        Self {
            field_name,
            max_length: Arc::new(max_length),
            required: Arc::new(AtomicBool::new(true)),
            value: Arc::new(Mutex::new(None)),
            error_handler: None,
            default_value: None,
        }
    }

    pub fn set_optional(self) -> Self {
        self.required.store(false, Ordering::Relaxed);
        self
    }

    pub fn set_default<S: AsRef<str>>(mut self, value: S) -> Self {
        let value = value.as_ref().to_string();
        self.default_value = Some(value);
        self
    }

    pub fn handle_error_message(
        mut self,
        callback: fn(InputFieldError, Vec<String>) -> Vec<String>,
    ) -> Self {
        let callback = Arc::new(Box::new(callback));
        self.error_handler = Some(callback);
        self
    }

    pub async fn value(&self) -> Option<String> {
        let value_ref = self.value.clone();
        let mut lock = value_ref.lock().await;
        lock.take()
    }
}

impl AbstractFields for InputField {
    fn field_name(&self) -> FieldResult<String> {
        let field_name = self.field_name.clone();
        Box::new(Box::pin(async move { field_name }))
    }

    fn validate(
        &mut self,
        form_data: &mut FormData,
        _: &mut Files,
    ) -> FieldResult<Result<(), Vec<String>>> {
        let field_name = self.field_name.clone();

        let form_value;

        // Takes value from form field
        if let Some(mut values) = form_data.remove(&field_name) {
            form_value = Some(values.remove(0));
        } else {
            form_value = None;
        }

        let required_ref = self.required.clone();
        let value_ref = self.value.clone();
        let max_length = self.max_length.clone();
        let default_value = self.default_value.take();

        let error_handler = self.error_handler.clone();

        Box::new(Box::pin(async move {
            let required = required_ref.load(Ordering::Relaxed);
            let mut errors: Vec<String> = vec![];

            if let Some(value) = form_value {
                // Handles value constraints

                if value.len() > *max_length {
                    // Checks maximum value length constraints
                    let default_max_length_exceed_messsage =
                        format!("Character length exceeds maximum size of {}", *max_length);

                    if let Some(error_handler) = error_handler {
                        let max_length_exceed_error =
                            InputFieldError::MaximumLengthExceed(&value, &field_name, &max_length);

                        let custom_errors = error_handler(
                            max_length_exceed_error,
                            vec![default_max_length_exceed_messsage],
                        );
                        errors.extend(custom_errors);
                    } else {
                        errors.push(default_max_length_exceed_messsage);
                    }
                }
                let mut lock = value_ref.lock().await;
                *lock = Some(value);
            } else {
                if required {
                    // Handles field missing error.
                    let default_field_missing_error = "This field is missing.".to_string();

                    if let Some(error_handler) = error_handler {
                        let field_missing_error = InputFieldError::MissingField(&field_name);
                        let custom_errors =
                            error_handler(field_missing_error, vec![default_field_missing_error]);
                        errors.extend(custom_errors);
                    } else {
                        errors.push(default_field_missing_error);
                    }
                } else {
                    let mut lock = value_ref.lock().await;
                    *lock = default_value;
                }
            }

            if errors.len() > 0 {
                return Err(errors);
            }

            Ok(())
        }))
    }

    fn wrap(&self) -> Box<dyn AbstractFields> {
        Box::new(self.clone())
    }
}
