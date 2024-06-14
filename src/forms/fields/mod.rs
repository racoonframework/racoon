pub mod file_field;
pub mod input_field;
pub mod uuid_field;

use std::future::Future;

use crate::core::forms::{Files, FormData};

type FieldResult<T> = Box<dyn Future<Output = T> + Send + Sync + Unpin>;

pub trait AbstractFields: Sync + Send {
    fn field_name(&self) -> FieldResult<String>;
    fn validate(
        &mut self,
        form_data: &mut FormData,
        files: &mut Files,
    ) -> FieldResult<Result<(), Vec<String>>>;
    fn wrap(&self) -> Box<dyn AbstractFields>;
}

pub type FormFields = Vec<Box<dyn AbstractFields + Sync + Send>>;

pub enum FieldError {
    Message(Vec<String>),
}
