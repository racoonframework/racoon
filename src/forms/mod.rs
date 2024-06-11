pub mod fields;

use std::collections::HashMap;
use std::future::Future;
use std::vec;

use crate::core::forms::FormFieldError;
use crate::core::request::Request;

use crate::forms::fields::AbstractFields;

pub type FormFields = Vec<Box<dyn AbstractFields + Sync + Send>>;

pub struct ValidationError {
    pub field_errors: HashMap<String, Vec<String>>,
    pub others: Vec<String>,
}

pub trait FormValidator: Sized + Send {
    fn new() -> Self;
    fn form_fields(&mut self) -> FormFields;
    fn validate<'a>(
        mut self,
        request: &'a Request,
    ) -> Box<dyn Future<Output = Result<Self, ValidationError>> + Sync + Send + Unpin + 'a>
    where
        Self: 'a,
        Self: Sync,
    {
        let request = request.clone();

        Box::new(Box::pin(async move {
            let mut field_errors: HashMap<String, Vec<String>> = HashMap::new();
            let mut other_errors = vec![];

            let (mut form_data, mut files) = match request
                .parse_body(request.form_constraints.clone())
                .await
            {
                Ok((form_data, files)) => (form_data, files),
                Err(error) => {
                    match error {
                        FormFieldError::MaxBodySizeExceed => {
                            other_errors.push("Max body size exceed.");
                        }

                        FormFieldError::MaxHeaderSizeExceed => {
                            other_errors.push("Max header size exceed.");
                        }

                        FormFieldError::MaxFileSizeExceed(field_name) => {
                            let file_size_exceed_error = vec!["Max file size exceed.".to_string()];
                            if let Some(errors) = field_errors.get_mut(&field_name) {
                                errors.extend_from_slice(&file_size_exceed_error);
                            } else {
                                field_errors.insert(field_name, file_size_exceed_error);
                            }
                        }

                        FormFieldError::MaxValueSizeExceed(field_name) => {
                            let value_length_exceed_error = vec!["Max value length exceed.".to_string()];
                            if let Some(errors) = field_errors.get_mut(&field_name) {
                                errors.extend_from_slice(&value_length_exceed_error);
                            } else {
                                field_errors.insert(field_name, value_length_exceed_error);
                            }
                        }
                        
                        FormFieldError::Others(field_name, error)=> {
                            if let Some(field_name) = field_name {
                                field_errors.insert(field_name, vec![error]);
                            } else {
                                other_errors.push(&error);
                            }
                        }
                    }
                    let validation_error = ValidationError {
                        field_errors: HashMap::new(),
                        others: vec![],
                    };
                    return Err(validation_error);
                }
            };

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
                        field_errors.insert(field_name, error);
                    }
                }
            }

            if field_errors.len() > 0 {
                let validation_error = ValidationError {
                    field_errors,
                    others: vec![],
                };
                return Err(validation_error);
            }

            Ok(self)
        }))
    }

    fn custom_validate(
        &mut self,
        _: &Request,
        _: &String,
        _: &Box<dyn AbstractFields + Sync + Send>,
    ) -> Box<dyn Future<Output = Option<Result<(), Vec<String>>>> + Sync + Send + Unpin + 'static>
    {
        Box::new(Box::pin(async move { None }))
    }
}
