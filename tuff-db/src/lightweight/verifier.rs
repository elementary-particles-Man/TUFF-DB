use std::collections::HashMap;
use std::net::Shutdown;
use std::net::TcpStream;

pub const TAG_KEY_MAX_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningMatchMode {
    Exact,
    Contains,
}

pub fn normalize_tag_key(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;

    for ch in input.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
            continue;
        }
        if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }

    let normalized = out.trim_matches('-').to_string();
    if normalized.is_empty() {
        return None;
    }

    let mut shortened = normalized.chars().take(TAG_KEY_MAX_LEN).collect::<String>();
    shortened = shortened.trim_matches('-').to_string();
    if shortened.is_empty() {
        None
    } else {
        Some(shortened)
    }
}

fn match_mode_for_tag(tag: &str) -> MeaningMatchMode {
    if tag == "id" || tag.ends_with("-id") || tag.contains("-id-") {
        MeaningMatchMode::Exact
    } else {
        MeaningMatchMode::Contains
    }
}

fn meaning_matches(mode: MeaningMatchMode, required: &str, payload: &str) -> bool {
    match mode {
        MeaningMatchMode::Exact => payload.trim() == required.trim(),
        MeaningMatchMode::Contains => payload.contains(required),
    }
}

#[derive(Debug, Clone)]
pub struct MeaningDb {
    meanings: HashMap<String, String>,
}

impl MeaningDb {
    pub fn new(raw_meanings: HashMap<String, String>) -> Self {
        let mut meanings = HashMap::new();
        for (raw_tag, meaning) in raw_meanings {
            if let Some(tag) = normalize_tag_key(&raw_tag) {
                meanings.insert(tag, meaning);
            }
        }
        Self { meanings }
    }

    pub fn meaning_for(&self, tag: &str) -> Option<&str> {
        let key = normalize_tag_key(tag)?;
        self.meanings.get(&key).map(|s| s.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Verifier {
    meaning_db: MeaningDb,
}

impl Verifier {
    pub fn new(meaning_db: MeaningDb) -> Self {
        Self { meaning_db }
    }

    pub fn verify_or_disconnect(
        &self,
        tag: &str,
        payload: &str,
        stream: &TcpStream,
    ) -> bool {
        let Some(normalized_tag) = normalize_tag_key(tag) else {
            let _ = stream.shutdown(Shutdown::Both);
            return false;
        };

        if let Some(required) = self.meaning_db.meaning_for(tag) {
            let mode = match_mode_for_tag(&normalized_tag);
            if !meaning_matches(mode, required, payload) {
                let _ = stream.shutdown(Shutdown::Both);
                return false;
            }
        }
        true
    }
}
