pub mod fields;

use std::collections::HashMap;
use std::future::Future;

use crate::core::request::Request;

use crate::forms::fields::AbstractFields;

pub type FormFields = Vec<Box<dyn AbstractFields + Sync + Send>>;

pub type ValidationError = HashMap<String, Vec<String>>;

pub trait FormValidator: Sized + Send {
    fn new() -> Self;
    fn form_fields(&mut self) -> FormFields;
    fn validate<'a>(
        mut self,
        request: &'a Request,
    ) -> Box<dyn Future<Output = Result<Self, ValidationError>> + Sync + Send + Unpin + 'a>
    where
        Self: 'a, Self: Sync,
    {
        let request = request.clone();

        Box::new(Box::pin(async move {
            let (mut form_data, mut files) = request.parse().await;

            let mut errors = HashMap::new();

            for mut field in self.form_fields() {
                let field_name = field.fields().await;

                let result;
                if let Some(custom_validate_result) =
                    self.custom_validate(&request, &field_name, &field).await
                {
                    result = custom_validate_result;
                } else {
                    result = field.validate(&mut form_data, &mut files).await;
                }

                match result {
                    Ok(()) => {}
                    Err(error) => {
                        errors.insert(field_name, error);
                    }
                }
            }

            if errors.len() > 0 {
                return Err(errors);
            }

            Ok(self)
        }))
    }

    fn custom_validate(
        &mut self,
        _: &Request,
        _: &String,
        _: &Box<dyn AbstractFields + Sync + Send>,
    ) -> Box<dyn Future<Output = Option<Result<(), Vec<String>>>> + Sync + Send + Unpin + 'static> {
        Box::new(Box::pin(async move { None }))
    }
}
