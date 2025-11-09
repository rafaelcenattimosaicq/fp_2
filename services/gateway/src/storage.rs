use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct HistoryDb {
    _placeholder: Arc<Mutex<()>>,
}

impl HistoryDb {
    pub fn open() -> Self {
        Self { _placeholder: Arc::new(Mutex::new(())) }
    }

    pub fn insert_poll(&self, _values: &std::collections::HashMap<String, crate::device_descriptor::RegisterValue>) {
        // TODO: persist to sqlite
    }
}
