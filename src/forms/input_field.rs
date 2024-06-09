use std::future::Future;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::forms::{Files, FormData};
use crate::core::shortcuts::SingleText;

#[derive(Clone)]
pub struct InputField {
    field_name: String,
    max_length: Arc<usize>,
    required: Arc<AtomicBool>,
    value: Arc<Mutex<Option<String>>>,
}

impl InputField {
    pub fn with<S: AsRef<str>>(field_name: S, max_length: usize) -> Self {
        let field_name = field_name.as_ref().to_string();

        Self {
            field_name,
            max_length: Arc::new(max_length),
            required: Arc::new(AtomicBool::new(true)),
            value: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_optional(self) -> Self {
        self.required.store(false, Ordering::Relaxed);
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
    ) -> FieldResult<Result<(), String>> {
        let field_name = self.field_name.clone();
        let max_length = self.max_length.clone();

        let value;

        // Takes value from form field
        if let Some(mut values) = form_data.remove(&field_name) {
            value = Some(values.remove(0));
        } else {
            value = None;
        }

        let value_ref = self.value.clone();

        Box::new(Box::pin(async move {
            if value.len() > *max_length {
                return Err(format!(
                    "Text exceeds {} characters length.",
                    *self.max_length
                ));
            }

            Ok(())
        }))
    }
}
