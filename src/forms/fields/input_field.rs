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

pub type ErrorHandler = Box<fn(InputFieldError, Vec<String>) -> Vec<String>>;

#[derive(Clone)]
pub struct InputField {
    field_name: String,
    max_length: Option<Arc<usize>>,
    required: Arc<AtomicBool>,
    values: Arc<Mutex<Option<Vec<String>>>>,
    error_handler: Option<Arc<ErrorHandler>>,
    default_value: Option<String>,
}

impl InputField {
    pub fn new<S: AsRef<str>>(field_name: S) -> Self {
        let field_name = field_name.as_ref().to_string();

        Self {
            field_name,
            max_length: None,
            required: Arc::new(AtomicBool::new(true)),
            values: Arc::new(Mutex::new(None)),
            error_handler: None,
            default_value: None,
        }
    }

    pub fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(Arc::new(max_length));
        self
    }

    pub fn set_optional(self) -> Self {
        self.required.store(false, Ordering::Relaxed);
        self
    }

    pub fn set_default<S: AsRef<str>>(self, value: S) -> Self {
        // Makes field optional
        let mut instance = self.set_optional();

        let value = value.as_ref().to_string();
        instance.default_value = Some(value);
        instance
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
        let value_ref = self.values.clone();
        let mut values = value_ref.lock().await;

        if let Some(values) = values.as_mut() {
            let value = values.remove(0);
            return Some(value);
        }

        None
    }

    pub async fn values(&self) -> Option<Vec<String>> {
        let value_ref = self.values.clone();
        let mut values = value_ref.lock().await;
        values.take()
    }

    fn validate_input_length(
        field_name: &String,
        values: &Vec<String>,
        error_handler: Option<Arc<ErrorHandler>>,
        max_length: Option<Arc<usize>>,
        errors: &mut Vec<String>,
    ) {
        let value;
        if let Some(value_ref) = values.get(0) {
            value = value_ref;
        } else {
            return;
        }

        if let Some(max_length) = max_length {
            // Checks maximum value length constraints
            if value.len() > *max_length {
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
        }
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

        let form_values;

        // Takes value from form field
        if let Some(values) = form_data.remove(&field_name) {
            form_values = Some(values);
        } else {
            form_values = None;
        }

        let required_ref = self.required.clone();
        let value_ref = self.values.clone();
        let max_length = self.max_length.clone();
        let default_value = self.default_value.take();

        let error_handler = self.error_handler.clone();

        Box::new(Box::pin(async move {
            let required = required_ref.load(Ordering::Relaxed);
            let mut errors: Vec<String> = vec![];

            let is_empty;
            if let Some(values) = form_values {
                InputField::validate_input_length(
                    &field_name,
                    &values,
                    error_handler.clone(),
                    max_length,
                    &mut errors,
                );

                {
                    // Handles value constraints
                    let mut lock = value_ref.lock().await;
                    is_empty = values.is_empty();

                    if !is_empty {
                        *lock = Some(values);
                    }
                }
            } else {
                is_empty = true;
            }

            // Handles field missing error.
            if required && is_empty {
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
                if let Some(default_value) = default_value {
                    {
                        let mut lock = value_ref.lock().await;
                        let values = vec![default_value];
                        *lock = Some(values);
                    }
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

#[cfg(test)]
pub mod test {
    use crate::{
        core::forms::{Files, FormData},
        forms::fields::AbstractFields,
    };

    use super::InputField;

    #[tokio::test]
    async fn test_validate_default() {
        let mut form_data = FormData::new();
        let mut files = Files::new();

        let mut input_field = InputField::new("name")
            .set_default("John")
            .set_optional()
            .max_length(100);
        let result = input_field.validate(&mut form_data, &mut files).await;
        assert_eq!(true, result.is_ok());

        let value = input_field.value().await;
        assert_eq!(value, Some("John".to_string()));
    }
}
