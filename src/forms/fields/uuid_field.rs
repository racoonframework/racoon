use std::any::Any;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;

use uuid::Uuid;

use crate::core::forms::{Files, FormData};
use crate::forms::fields::{AbstractFields, FieldResult};

pub trait ToTypeT {
    fn from_vec(values: &mut Vec<String>) -> Option<Self>
    where
        Self: Sized;
}

impl ToTypeT for Uuid {
    fn from_vec(values: &mut Vec<String>) -> Option<Self>
    where
        Self: Sized,
    {
        if values.len() > 0 {
            let value = values.remove(0);
            match Uuid::parse_str(&value) {
                Ok(uuid) => return Some(uuid),
                _ => {}
            }
        }
        None
    }
}

type BoxResult = Box<dyn Any + Send + Sync>;

pub enum UuidFieldError<'a> {
    /// (field_name)
    MissingField(&'a String),
    /// (field_name, value)
    InvalidUuid(&'a String, &'a Vec<String>),
}

pub type ErrorHandler = Box<fn(UuidFieldError, Vec<String>) -> Vec<String>>;

pub struct UuidField<T> {
    field_name: String,
    result: Arc<Mutex<Option<BoxResult>>>,
    validated: Arc<AtomicBool>,
    error_handler: Option<Arc<ErrorHandler>>,
    phantom: PhantomData<T>,
}

impl<T> Clone for UuidField<T> {
    fn clone(&self) -> Self {
        Self {
            field_name: self.field_name.clone(),
            result: self.result.clone(),
            validated: self.validated.clone(),
            error_handler: self.error_handler.clone(),
            phantom: self.phantom.clone(),
        }
    }
}

impl<T: ToTypeT + Sync + Send> UuidField<T> {
    pub fn new<S: AsRef<str>>(field_name: S) -> Self {
        let field_name = field_name.as_ref().to_string();

        Self {
            field_name,
            result: Arc::new(Mutex::new(None)),
            validated: Arc::new(AtomicBool::new(false)),
            error_handler: None,
            phantom: PhantomData,
        }
    }

    pub fn handle_error_message(
        mut self,
        callback: fn(UuidFieldError, Vec<String>) -> Vec<String>,
    ) -> Self {
        self.error_handler = Some(Arc::new(Box::new(callback)));
        self
    }

    pub async fn value(self) -> T
    where
        T: 'static,
    {
        if !self.validated.load(Ordering::Relaxed) {
            panic!("This field is not validated. Please call form.validate() method before accessing value.");
        }

        let mut lock = self.result.lock().await;
        if let Some(result) = lock.take() {
            match result.downcast::<T>() {
                Ok(t) => {
                    return *t;
                }
                _ => {}
            };
        }
        panic!("Unexpected error. Bug in uuid_field.rs file.");
    }
}

impl<T: ToTypeT + Sync + Send + 'static> AbstractFields for UuidField<T> {
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
        let mut values = form_data.remove(&field_name);
        let result = self.result.clone();
        let validated = self.validated.clone();

        let error_handler = self.error_handler.clone();

        Box::new(Box::pin(async move {
            let is_empty;
            let is_optional = std::any::TypeId::of::<T>() == std::any::TypeId::of::<Option<Uuid>>();

            let mut errors: Vec<String> = vec![];

            if let Some(mut values) = values.as_mut() {
                is_empty = values.is_empty();
                let option_t = T::from_vec(&mut values);

                if let Some(t) = option_t {
                    let mut result = result.lock().await;
                    *result = Some(Box::new(t));
                } else {
                    let default_uuid_invalid_error = "Invalid UUId.".to_string();
                    if let Some(error_handler) = error_handler.clone() {
                        let invalid_uuid_error = UuidFieldError::InvalidUuid(&field_name, &values);
                        let custom_errors =
                            error_handler(invalid_uuid_error, vec![default_uuid_invalid_error]);
                        errors.extend_from_slice(&custom_errors);
                    } else {
                        errors.push(default_uuid_invalid_error);
                    }
                }
            } else {
                is_empty = true;
            }

            if !is_optional && is_empty {
                let default_uuid_missing_error = "This field is required.".to_string();

                if let Some(error_handler) = error_handler.clone() {
                    let uuid_missing_error = UuidFieldError::MissingField(&field_name);
                    let custom_errors =
                        error_handler(uuid_missing_error, vec![default_uuid_missing_error]);
                    errors.extend_from_slice(&custom_errors);
                } else {
                    errors.push(default_uuid_missing_error);
                }
            }

            if errors.len() > 0 {
                return Err(errors);
            }

            validated.store(true, Ordering::Relaxed);
            Ok(())
        }))
    }

    fn wrap(&self) -> Box<dyn AbstractFields> {
        Box::new(self.clone())
    }
}
