use crate::models::{Abstract, VerificationStatus};
use std::collections::HashMap;

#[derive(Default, Debug)]
pub struct InMemoryIndex {
    by_tag_key: HashMap<String, Vec<Abstract>>,
}

impl InMemoryIndex {
    pub fn insert(&mut self, abstract_: Abstract) {
        let key = abstract_.tags.to_key();
        self.by_tag_key.entry(key).or_default().push(abstract_);
    }

    pub fn select(
        &self,
        tag_key: Option<&str>,
        min_verification: Option<VerificationStatus>,
    ) -> Vec<Abstract> {
        let mut results: Vec<Abstract> = Vec::new();
        match tag_key {
            Some(key) => {
                if let Some(list) = self.by_tag_key.get(key) {
                    results.extend(list.iter().cloned());
                }
            }
            None => {
                for list in self.by_tag_key.values() {
                    results.extend(list.iter().cloned());
                }
            }
        }
        if let Some(min) = min_verification {
            results.retain(|a| a.verification >= min);
        }
        results
    }
}
