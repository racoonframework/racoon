use std::any::Any;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::forms::{Files, FormData};
use crate::forms::AbstractFields;

use crate::forms::fields::FieldResult;

pub struct UploadedFile {
    pub filename: String,
    pub temp_path: PathBuf,
}

impl UploadedFile {
    pub fn from_core_file_field(file_field: crate::core::forms::FileField) -> Self {
        let temp_path = file_field.temp_file.path().to_owned();

        Self {
            filename: file_field.name,
            temp_path,
        }
    }
}

type BoxResult = Box<dyn Any + Sync + Send + 'static>;

pub struct FileField<T> {
    field_name: String,
    result: Arc<Mutex<Option<BoxResult>>>,
    validated: Arc<AtomicBool>,
    phantom: PhantomData<T>,
}

impl<T> Clone for FileField<T> {
    fn clone(&self) -> Self {
        Self {
            field_name: self.field_name.clone(),
            result: self.result.clone(),
            validated: self.validated.clone(),
            phantom: self.phantom.clone(),
        }
    }
}

pub trait FromFileField {
    fn from_vec(files: &mut Vec<crate::core::forms::FileField>) -> Option<Self>
    where
        Self: Sized;
}

impl FromFileField for UploadedFile {
    fn from_vec(files: &mut Vec<crate::core::forms::FileField>) -> Option<Self> {
        if files.len() > 0 {
            let file_field = files.remove(0);
            return Some(UploadedFile::from_core_file_field(file_field));
        }

        None
    }
}

impl FromFileField for Option<UploadedFile> {
    fn from_vec(files: &mut Vec<crate::core::forms::FileField>) -> Option<Self> {
        if files.len() > 0 {
            let file_field = files.remove(0);
            // Outer Some denotes successful conversion.
            return Some(Some(UploadedFile::from_core_file_field(file_field)));
        }

        // Return successful conversion but no files are present. So returns actual value as None.
        Some(None)
    }
}

impl<T: Sync + Send + 'static> FileField<T> {
    pub fn new<S: AsRef<str>>(field_name: S) -> Self {
        let field_name = field_name.as_ref().to_string();
        Self {
            field_name,
            result: Arc::new(Mutex::new(None)),
            validated: Arc::new(AtomicBool::from(false)),
            phantom: PhantomData,
        }
    }

    pub async fn value(self) -> T {
        if !self.validated.load(Ordering::Relaxed) {
            panic!("This field is not validated. Please call form.validate() method before accessing value.");
        }

        let mut result_ref = self.result.lock().await;
        let result = result_ref.take();

        if let Some(result) = result {
            match result.downcast::<T>() {
                Ok(t) => {
                    let t = *t;
                    return t;
                }

                _ => {}
            };
        }

        panic!("Unexpected error. Bug in file_field.rs file.");
    }
}

impl<T: FromFileField + Sync + Send + 'static> AbstractFields for FileField<T> {
    fn field_name(&self) -> FieldResult<String> {
        let field_name = self.field_name.clone();
        Box::new(Box::pin(async move { field_name }))
    }

    fn validate(
        &mut self,
        _: &mut FormData,
        files: &mut Files,
    ) -> FieldResult<Result<(), Vec<String>>> {
        let files = files.remove(&self.field_name);
        let result_ref = self.result.clone();
        let validated = self.validated.clone();

        Box::new(Box::pin(async move {
            let mut errors = vec![];

            let is_optional =
                std::any::TypeId::of::<T>() == std::any::TypeId::of::<Option<UploadedFile>>();

            let is_empty;

            if let Some(mut files) = files {
                let result = result_ref.lock().await;
                let mut option = result;

                is_empty = files.is_empty();

                if let Some(t) = T::from_vec(&mut files) {
                    *option = Some(Box::new(t));
                }
            } else {
                is_empty = true;
            }

            if !is_optional && is_empty {
                errors.push("This field is required.".to_string());
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
