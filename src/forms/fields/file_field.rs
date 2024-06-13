use std::marker::PhantomData;

use crate::core::forms::{Files, FormData};
use crate::forms::AbstractFields;

use crate::forms::fields::FieldResult;

pub struct FileField<T> {
    field_name: String,
    phantom: PhantomData<T>,
}

impl<T: Sync + Send> FileField<T> {
    pub fn new<S: AsRef<str>>(field_name: S) -> Self {
        let field_name = field_name.as_ref().to_string();
        Self {
            field_name,
            phantom: PhantomData,
        }
    }
}

impl<T: Sync + Send> AbstractFields for FileField<T> {
    fn field_name(&self) -> super::FieldResult<String> {
        todo!()
    }

    fn validate(
        &mut self,
        form_data: &mut FormData,
        files: &mut Files,
    ) -> FieldResult<Result<(), Vec<String>>> {
        todo!()
    }

    fn wrap(&self) -> Box<dyn AbstractFields> {
        todo!()
    }
}
