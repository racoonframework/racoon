pub mod input_field;

use std::future::Future;

use crate::core::forms::{Files, FormData};

pub type Fields<T> = Box<dyn Future<Output = T> + Send + Sync + Unpin>;

pub trait AbstractFields: Sync + Send {
    fn fields(&self) -> Fields<String>;
    fn validate(
        &mut self,
        form_data: &mut FormData,
        files: &mut Files,
    ) -> Fields<Result<(), Vec<String>>>;
    fn wrap(&self) -> Box<dyn AbstractFields>;
}

pub type FormFields = Vec<Box<dyn AbstractFields + Sync + Send>>;

pub enum FieldError {
    Message(Vec<String>),
}
