use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct WalStorage {
    path: PathBuf,
    index: HashMap<String, Vec<u64>>,
}

impl WalStorage {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            File::create(&path)?;
        }
        let mut storage = Self {
            path,
            index: HashMap::new(),
        };
        storage.rebuild_index()?;
        Ok(storage)
    }

    pub fn append(&mut self, tag: &str, payload: &str) -> io::Result<u64> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let offset = file.metadata()?.len();
        let escaped = escape_payload(payload);
        let checksum = sha256_hex(&format!("{tag}\t{escaped}"));
        let line = format!("{tag}\t{escaped}\t{checksum}\n");
        file.write_all(line.as_bytes())?;
        file.flush()?;
        self.index.entry(tag.to_string()).or_default().push(offset);
        Ok(offset)
    }

    pub fn select_offsets(&self, tag: &str) -> Vec<u64> {
        self.index.get(tag).cloned().unwrap_or_default()
    }

    pub fn read_at_offset(&self, offset: u64) -> io::Result<Option<WalRecord>> {
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        Ok(parse_line(&line))
    }

    fn rebuild_index(&mut self) -> io::Result<()> {
        self.index.clear();
        let file = File::open(&self.path)?;
        let mut reader = BufReader::new(file);
        let mut offset: u64 = 0;
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                break;
            }
            if let Some(record) = parse_line(&line) {
                let expected = sha256_hex(&format!("{}\t{}", record.tag, escape_payload(&record.payload)));
                if expected == record.checksum {
                    self.index
                        .entry(record.tag.clone())
                        .or_default()
                        .push(offset);
                }
            }
            offset += bytes as u64;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct WalRecord {
    pub tag: String,
    pub payload: String,
    pub checksum: String,
}

fn parse_line(line: &str) -> Option<WalRecord> {
    let mut parts = line.trim_end().splitn(3, '\t');
    let tag = parts.next()?.to_string();
    let payload_escaped = parts.next()?.to_string();
    let checksum = parts.next()?.to_string();
    let payload = unescape_payload(&payload_escaped);
    Some(WalRecord {
        tag,
        payload,
        checksum,
    })
}

fn escape_payload(input: &str) -> String {
    input.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unescape_payload(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('t') => {
                    chars.next();
                    out.push('\t');
                }
                Some('n') => {
                    chars.next();
                    out.push('\n');
                }
                Some('\\') => {
                    chars.next();
                    out.push('\\');
                }
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}
