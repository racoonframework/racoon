use std::collections::HashMap;

pub trait SingleText {
    /// Performs case-insensitive lookup and returns first value found.
    fn value<S: AsRef<str>>(&self, name: S) -> Option<&String>;
}


impl SingleText for HashMap<String, Vec<String>> {
    fn value<S: AsRef<str>>(&self, name: S) -> Option<&String> {
        let name = name.as_ref();

        for (key, values) in self.iter() {
            if key.to_lowercase() != name.to_lowercase() {
                continue;
            }

            if let Some(field) = values.get(0) {
                return Some(field);
            }
        }
        None
    }
}
