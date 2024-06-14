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

pub struct UuidField<T> {
    field_name: String,
    result: Arc<Mutex<Option<BoxResult>>>,
    validated: Arc<AtomicBool>,
    phantom: PhantomData<T>,
}

impl<T> Clone for UuidField<T> {
    fn clone(&self) -> Self {
        Self {
            field_name: self.field_name.clone(),
            result: self.result.clone(),
            validated: self.validated.clone(),
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
            phantom: PhantomData,
        }
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
                    errors.push("Invalid UUID.".to_string());
                }
            } else {
                is_empty = true;
            }

            if !is_optional && is_empty {
                errors.push("This field is required".to_string());
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
