use std::collections::HashMap;
use std::fs;
use std::path::Path;

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
pub struct TagIndex {
    map: HashMap<String, String>,
}

impl TagIndex {
    pub fn from_map(raw: HashMap<String, String>) -> Self {
        let mut map = HashMap::new();
        for (tag, meaning) in raw {
            if let Some(key) = normalize_tag_key(&tag) {
                map.insert(key, meaning);
            }
        }
        Self { map }
    }

    pub fn get(&self, tag: &str) -> Option<&str> {
        let key = normalize_tag_key(tag)?;
        self.map.get(&key).map(|s| s.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

#[derive(Debug, Clone)]
pub struct MeaningDb {
    tag_index: TagIndex,
}

impl MeaningDb {
    pub fn new(raw_meanings: HashMap<String, String>) -> Self {
        Self {
            tag_index: TagIndex::from_map(raw_meanings),
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let content = fs::read_to_string(path)?;
        let mut map = HashMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, '=');
            let tag = match parts.next() {
                Some(v) => v.trim(),
                None => continue,
            };
            let meaning = match parts.next() {
                Some(v) => v.trim(),
                None => continue,
            };
            if !tag.is_empty() && !meaning.is_empty() {
                map.insert(tag.to_string(), meaning.to_string());
            }
        }
        Ok(Self::new(map))
    }

    pub fn merge(&mut self, raw_meanings: HashMap<String, String>) {
        let mut merged = HashMap::new();
        for (k, v) in self.tag_index.iter() {
            merged.insert(k.to_string(), v.to_string());
        }
        for (k, v) in raw_meanings {
            merged.insert(k, v);
        }
        self.tag_index = TagIndex::from_map(merged);
    }

    pub fn meaning_for(&self, tag: &str) -> Option<&str> {
        self.tag_index.get(tag)
    }
}

#[derive(Debug, Clone)]
pub struct LightweightHit {
    pub tag: String,
    pub required: String,
    pub mode: MeaningMatchMode,
}

#[derive(Debug, Clone)]
pub struct LightweightVerifier {
    meaning_db: MeaningDb,
}

impl LightweightVerifier {
    pub fn new(meaning_db: MeaningDb) -> Self {
        Self { meaning_db }
    }

    pub fn from_sources(path: Option<&Path>, env_pairs: HashMap<String, String>) -> Option<Self> {
        let mut db = path
            .and_then(|p| MeaningDb::from_path(p).ok())
            .unwrap_or_else(|| MeaningDb::new(HashMap::new()));
        db.merge(env_pairs);
        if db.tag_index.iter().next().is_none() {
            None
        } else {
            Some(Self::new(db))
        }
    }

    pub fn verify_fragment(&self, fragment: &str) -> Option<LightweightHit> {
        let (tag, payload) = split_tag_payload(fragment);
        self.verify_tag_payload(&tag, &payload)
    }

    pub fn verify_tag_payload(&self, tag: &str, payload: &str) -> Option<LightweightHit> {
        let normalized_tag = normalize_tag_key(tag)?;
        let required = self.meaning_db.meaning_for(&normalized_tag)?;
        let mode = match_mode_for_tag(&normalized_tag);
        if meaning_matches(mode, required, payload) {
            Some(LightweightHit {
                tag: normalized_tag,
                required: required.to_string(),
                mode,
            })
        } else {
            None
        }
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

    pub fn verify_tag_payload(&self, tag: &str, payload: &str) -> bool {
        let lw = LightweightVerifier::new(self.meaning_db.clone());
        self.meaning_db.meaning_for(tag).is_none() || lw.verify_tag_payload(tag, payload).is_some()
    }
}

fn split_tag_payload(line: &str) -> (String, String) {
    if let Some((tag, payload)) = line.split_once('\t') {
        return (tag.trim().to_string(), payload.trim().to_string());
    }
    if let Some((tag, payload)) = line.split_once(' ') {
        return (tag.trim().to_string(), payload.trim().to_string());
    }
    (line.trim().to_string(), String::new())
}
