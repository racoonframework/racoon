pub mod input_field;

use std::future::Future;

use crate::core::forms::{Files, FormData};

pub type FieldResult<T> = Box<dyn Future<Output = T> + Sync + Unpin>;

pub trait AbstractFields {
    fn field_name(&self) -> FieldResult<String>;
    fn validate(
        &mut self,
        form_data: &mut FormData,
        files: &mut Files,
    ) -> FieldResult<Result<(), Vec<String>>>;
    fn wrap(&self) -> Box<dyn AbstractFields>;
}

pub type FormFields = Vec<Box<dyn AbstractFields>>;

pub enum FieldError {
    Message(Vec<String>),
}
