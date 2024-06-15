use std::any::Any;
use std::marker::PhantomData;
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

pub type PostValidator<T> = Box<fn(T) -> Result<T, Vec<String>>>;
pub type ErrorHandler = Box<fn(InputFieldError, Vec<String>) -> Vec<String>>;

pub trait ToOptionT {
    fn from_vec(value: &mut Vec<String>) -> Option<Self>
    where
        Self: Sized;
    fn is_optional() -> bool;
}

impl ToOptionT for String {
    fn from_vec(values: &mut Vec<String>) -> Option<Self> {
        if values.len() > 0 {
            return Some(values.remove(0));
        }

        // Here None denotes values cannot be correctly converted to type T.
        None
    }

    fn is_optional() -> bool {
        false
    }
}

impl ToOptionT for Option<String> {
    fn from_vec(values: &mut Vec<String>) -> Option<Self> {
        if values.len() > 0 {
            let value = values.remove(0);
            return Some(Some(value));
        } else {
            // Here outer Some denotes values are correctly converted to type T with value None.
            // Since fields are missing, default value is None.
            return Some(None);
        }
    }

    fn is_optional() -> bool {
        true
    }
}

impl ToOptionT for Vec<String> {
    fn from_vec(values: &mut Vec<String>) -> Option<Self> {
        // At least one value must be present to be a required field.
        if values.len() > 0 {
            let mut owned_values = vec![];

            for i in (0..values.len()).rev() {
                owned_values.push(values.remove(i));
            }

            return Some(owned_values);
        }

        // Here None denotes values cannot be correctly converted to type T.
        None
    }

    fn is_optional() -> bool {
        false
    }
}

impl ToOptionT for Option<Vec<String>> {
    fn from_vec(values: &mut Vec<String>) -> Option<Self> {
        // At least one value must be present to be a required field.
        if values.len() > 0 {
            let mut owned_values = vec![];

            for i in (0..values.len()).rev() {
                owned_values.push(values.remove(i));
            }

            return Some(Some(owned_values));
        }

        // Here no values are received but since it's optional field,
        // returns successfull conversion to type None.
        Some(None)
    }

    fn is_optional() -> bool {
        true
    }
}

type BoxResult = Box<dyn Any + Send + Sync + 'static>;

pub struct InputField<T> {
    field_name: String,
    /// Maximum allowed text size.
    max_length: Option<Arc<usize>>,
    /// Minimum length size for valid input field.
    min_length: Option<Arc<usize>>,
    /// Option enum holds the value of type T.
    result: Arc<Mutex<Option<BoxResult>>>,
    /// Custom function callback for handling error.
    error_handler: Option<Arc<ErrorHandler>>,
    /// Custom callback for post field validation.
    post_validator: Option<Arc<PostValidator<T>>>,
    /// Default value if no form field value received.
    default_value: Option<String>,
    /// True if validated successfully else false.
    validated: Arc<AtomicBool>,
    /// Dummy type for compile time and runtime check.
    phantom: PhantomData<T>,
}

impl<T: ToOptionT + Sync + Send + 'static> InputField<T> {
    pub fn new<S: AsRef<str>>(field_name: S) -> Self {
        let field_name = field_name.as_ref().to_string();

        Self {
            field_name,
            max_length: None,
            min_length: None,
            result: Arc::new(Mutex::new(None)),
            error_handler: None,
            post_validator: None,
            default_value: None,
            validated: Arc::new(AtomicBool::from(false)),
            phantom: PhantomData,
        }
    }

    pub fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(Arc::new(max_length));
        self
    }

    pub fn min_length(mut self, min_length: usize) -> Self {
        self.min_length = Some(Arc::new(min_length));
        self
    }

    pub fn set_default<S: AsRef<str>>(mut self, value: S) -> Self {
        let value = value.as_ref().to_string();
        self.default_value = Some(value);
        self
    }

    pub fn post_validate(mut self, call: fn(t: T) -> Result<T, Vec<String>>) -> Self {
        self.post_validator = Some(Arc::new(Box::new(call)));
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

    pub async fn value(self) -> T {
        if !self.validated.load(Ordering::Relaxed) {
            panic!("This field is not validated. Please call form.validate() method before accessing value.");
        }

        let mut result_ref = self.result.lock().await;
        let result = result_ref.take();

        if let Some(result) = result {
            match result.downcast::<T>() {
                Ok(t) => {
                    return *t;
                }

                _ => {}
            };
        }

        panic!("Unexpected error. Bug in input_field.rs file.");
    }
}
fn validate_input_length(
    field_name: &String,
    values: &Vec<String>,
    error_handler: Option<Arc<ErrorHandler>>,
    max_length: Option<Arc<usize>>,
    min_length: Option<Arc<usize>>,
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

            if let Some(error_handler) = error_handler.clone() {
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

    if let Some(min_length) = min_length {
        // Checks maximum value length constraints
        if value.len() < *min_length {
            let default_max_length_exceed_messsage =
                format!("Text length is less then {}", *min_length);

            if let Some(error_handler) = error_handler.clone() {
                let max_length_exceed_error =
                    InputFieldError::MinimumLengthRequired(&value, &field_name, &min_length);

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

impl<T: ToOptionT> Clone for InputField<T> {
    fn clone(&self) -> Self {
        Self {
            field_name: self.field_name.clone(),
            max_length: self.max_length.clone(),
            min_length: self.min_length.clone(),
            error_handler: self.error_handler.clone(),
            post_validator: self.post_validator.clone(),
            result: self.result.clone(),
            default_value: self.default_value.clone(),
            validated: self.validated.clone(),
            phantom: self.phantom.clone(),
        }
    }
}

impl<T: ToOptionT + Sync + Send + 'static> AbstractFields for InputField<T> {
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

        let mut form_values;

        // Takes value from form field
        if let Some(values) = form_data.remove(&field_name) {
            form_values = Some(values);
        } else {
            form_values = None;
        }

        let max_length = self.max_length.clone();
        let min_length = self.min_length.clone();
        let default_value = self.default_value.take();
        let validated = self.validated.clone();
        let result = self.result.clone();

        let error_handler = self.error_handler.clone();
        let post_validator = self.post_validator.clone();

        Box::new(Box::pin(async move {
            let mut errors: Vec<String> = vec![];

            let is_empty;
            if let Some(values) = form_values.as_mut() {
                validate_input_length(
                    &field_name,
                    &values,
                    error_handler.clone(),
                    max_length,
                    min_length,
                    &mut errors,
                );

                is_empty = values.is_empty();
            } else {
                is_empty = true;
            }

            // Handles field missing error.
            let is_optional = T::is_optional();

            if !is_optional && is_empty {
                // If default value is specified, set default value for value
                if let Some(default_value) = default_value {
                    if is_empty {
                        form_values = Some(vec![default_value]);
                    }
                } else {
                    let default_field_missing_error = "This field is missing.".to_string();

                    if let Some(error_handler) = error_handler {
                        let field_missing_error = InputFieldError::MissingField(&field_name);
                        let custom_errors =
                            error_handler(field_missing_error, vec![default_field_missing_error]);
                        errors.extend(custom_errors);
                    } else {
                        errors.push(default_field_missing_error);
                    }
                }
            }

            if errors.len() > 0 {
                return Err(errors);
            }

            // All the validation conditions are satisfied.
            {
                let mut result_lock = result.lock().await;
                if let Some(values) = form_values.as_mut() {
                    let value_t = T::from_vec(values);
                    if let Some(mut t) = value_t {
                        if let Some(post_validator) = post_validator {
                            // Performs post validation callback.
                            match post_validator(t) {
                                Ok(post_validated_t) => {
                                    t = post_validated_t;
                                    *result_lock = Some(Box::new(t));
                                }
                                Err(custom_errors) => {
                                    return Err(custom_errors);
                                }
                            }
                        } else {
                            *result_lock = Some(Box::new(t));
                        };
                    }
                } else {
                    // Above conditions are satisfied however there are no values stored.
                    // Probably Optional type without default value.
                    let value_t = T::from_vec(&mut vec![]);
                    *result_lock = Some(Box::new(value_t.unwrap()));
                }
            }

            validated.store(true, Ordering::Relaxed);
            Ok(())
        }))
    }

    fn wrap(&self) -> Box<dyn AbstractFields> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
pub mod test {
    use crate::core::forms::{Files, FormData};
    use crate::forms::fields::AbstractFields;

    use super::InputField;

    #[tokio::test]
    async fn test_validate_default() {
        let mut form_data = FormData::new();
        let mut files = Files::new();

        let mut input_field: InputField<String> =
            InputField::new("name").set_default("John").max_length(100);
        let result = input_field.validate(&mut form_data, &mut files).await;
        assert_eq!(true, result.is_ok());

        let value = input_field.value().await;
        assert_eq!(value, "John");
    }

    #[tokio::test]
    async fn test_validate_string() {
        let mut form_data = FormData::new();
        form_data.insert("name".to_string(), vec!["John".to_string()]);

        let mut files = Files::new();

        let mut input_field: InputField<String> = InputField::new("name").max_length(100);
        let result = input_field.validate(&mut form_data, &mut files).await;
        assert_eq!(true, result.is_ok());

        let value = input_field.value().await;
        assert_eq!(value, "John");
    }

    #[tokio::test]
    async fn test_validate_optional() {
        let mut form_data = FormData::new();
        let mut files = Files::new();

        let mut input_field: InputField<Option<String>> = InputField::new("name").max_length(100);
        let result = input_field.validate(&mut form_data, &mut files).await;
        assert_eq!(true, result.is_ok());

        let value = input_field.value().await;
        assert_eq!(value, None);

        // With values
        form_data.insert("name".to_string(), vec!["John".to_string()]);
        let mut input_field2: InputField<Option<String>> = InputField::new("name").max_length(100);
        let result = input_field2.validate(&mut form_data, &mut files).await;
        assert_eq!(true, result.is_ok());
        assert_eq!(Some("John".to_string()), input_field2.value().await);
    }

    #[tokio::test]
    async fn test_validate_vec() {
        let mut form_data = FormData::new();
        let mut files = Files::new();

        let mut input_field: InputField<Vec<String>> = InputField::new("name").max_length(100);
        let result = input_field.validate(&mut form_data, &mut files).await;
        assert_eq!(false, result.is_ok());

        // With values
        let mut input_field2: InputField<Vec<String>> = InputField::new("name").max_length(100);

        form_data.insert(
            "name".to_string(),
            vec![
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
            ],
        );

        let result = input_field2.validate(&mut form_data, &mut files).await;
        assert_eq!(true, result.is_ok());
        assert_eq!(4, input_field2.value().await.len());
    }

    #[tokio::test]
    async fn test_validate_vec_optional() {
        let mut form_data = FormData::new();
        let mut files = Files::new();

        let mut input_field: InputField<Option<Vec<String>>> =
            InputField::new("name").max_length(100);
        let result = input_field.validate(&mut form_data, &mut files).await;
        assert_eq!(true, result.is_ok());
        assert_eq!(false, input_field.value().await.is_some());

        // With values
        let mut input_field2: InputField<Option<Vec<String>>> =
            InputField::new("name").max_length(100);

        form_data.insert(
            "name".to_string(),
            vec![
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
            ],
        );

        let result = input_field2.validate(&mut form_data, &mut files).await;
        assert_eq!(true, result.is_ok());

        let value = input_field2.value().await;
        assert_eq!(true, value.is_some());
        assert_eq!(4, value.unwrap().len());
    }

    #[tokio::test]
    async fn test_value_length() {
        // Validate long text
        let mut input_field: InputField<String> = InputField::new("name").max_length(10);
        let mut form_data = FormData::new();

        const LONG_PARAGRAPH: &str = r#"
        Lorem ipsum dolor sit amet, qui minim labore adipisicing minim sint cillum sint consectetur cupidatat.
        "#;
        form_data.insert("name".to_string(), vec![LONG_PARAGRAPH.to_string()]);

        let mut files = Files::new();
        let result = input_field.validate(&mut form_data, &mut files).await;
        assert_eq!(false, result.is_ok());

        // Validate long text
        let mut input_field2: InputField<String> = InputField::new("name").min_length(100);
        let mut form_data = FormData::new();

        const SHORT_PARAGRAPH: &str = r#"
        Lorem ipsum dolor sit amet.
        "#;
        form_data.insert("name".to_string(), vec![SHORT_PARAGRAPH.to_string()]);

        let mut files = Files::new();
        let result = input_field2.validate(&mut form_data, &mut files).await;
        assert_eq!(false, result.is_ok());
    }

    #[tokio::test]
    async fn test_empty_value_with_length() {
        let mut input_field: InputField<String> = InputField::new("name").max_length(100);
        let mut form_data = FormData::new();
        let mut files = Files::new();
        let result = input_field.validate(&mut form_data, &mut files).await;
        assert_eq!(false, result.is_ok());
    }

    #[tokio::test]
    async fn test_post_validate() {
        let mut input_field: InputField<String> = InputField::new("name")
            .max_length(100)
            .post_validate(|value| {
                if !value.eq("John") {
                    return Err(vec!["Value is not John".to_string()]);
                }

                Ok(value)
            });
        let mut form_data = FormData::new();
        form_data.insert("name".to_string(), vec!["Raphel".to_string()]);

        let mut files = Files::new();
        let result = input_field.validate(&mut form_data, &mut files).await;
        assert_eq!(false, result.is_ok());
    }
}
