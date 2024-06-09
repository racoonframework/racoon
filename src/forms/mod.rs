use crate::core::request::Request;

use self::fields::AbstractFields;

pub mod fields;

pub type FormFields = Vec<Box<dyn AbstractFields>>;

pub trait FormValidator {
    fn new() -> Self;
    fn form_fields(&mut self) -> FormFields;
    fn validate(&mut self, request: &Request) {
        let request = request.clone();
        // let (form_data, files) = request.parse().await;
        //
        // for field in self.form_fields() {
        //
        //     // field.validate(, files);
        // }
    }
    fn custom_validate(&mut self, request: &Request) {}
}
