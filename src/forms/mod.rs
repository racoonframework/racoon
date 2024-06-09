pub mod fields;

use std::collections::HashMap;
use std::future::Future;

use crate::core::request::Request;

use self::fields::AbstractFields;

pub type FormFields = Vec<Box<dyn AbstractFields>>;

pub type ValidationError = HashMap<String, Vec<String>>;

pub trait FormValidator {
    fn new() -> Self;
    fn form_fields(&mut self) -> FormFields;
    fn validate(
        &mut self,
        request: &Request,
    ) -> Box<dyn Future<Output = Result<(), ValidationError>> + '_> {
        let request = request.clone();

        Box::new(Box::pin(async move {
            let (mut form_data, mut files) = request.parse().await;

            let mut errors = HashMap::new();

            for mut field in self.form_fields() {
                let field_name = field.field_name().await;

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

            Ok(())
        }))
    }

    fn custom_validate(
        &mut self,
        _: &Request,
        _: &String,
        _: &Box<dyn AbstractFields>,
    ) -> Box<dyn Future<Output = Option<Result<(), Vec<String>>>> + Sync + Unpin + 'static> {
        Box::new(Box::pin(async move { None }))
    }
}
