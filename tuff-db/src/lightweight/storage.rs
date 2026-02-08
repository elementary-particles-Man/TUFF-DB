use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};

#[derive(Debug, Clone, Copy)]
pub enum RecoveryMode {
    Strict,
    TruncateCorruptedTail,
}

#[derive(Debug)]
pub struct WalStorage {
    path: PathBuf,
    index: HashMap<String, Vec<u64>>,
}

impl WalStorage {
    pub async fn open(path: impl AsRef<Path>, mode: RecoveryMode) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::File::create(&path).await?;
        }

        let mut storage = Self {
            path,
            index: HashMap::new(),
        };
        storage.rebuild_index(mode).await?;
        Ok(storage)
    }

    pub async fn append(&mut self, tag: &str, payload: &str) -> io::Result<u64> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;

        let offset = file.metadata().await?.len();
        let escaped = escape_payload(payload);
        let checksum = sha256_hex(&format!("{tag}\t{escaped}"));
        let line = format!("{tag}\t{escaped}\t{checksum}\n");

        file.write_all(line.as_bytes()).await?;
        file.flush().await?;

        self.index.entry(tag.to_string()).or_default().push(offset);
        Ok(offset)
    }

    pub fn select_offsets(&self, tag: &str) -> Vec<u64> {
        self.index.get(tag).cloned().unwrap_or_default()
    }

    pub async fn read_at_offset(&self, offset: u64) -> io::Result<Option<WalRecord>> {
        let mut file = fs::File::open(&self.path).await?;
        file.seek(SeekFrom::Start(offset)).await?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await?;
        if buf.is_empty() {
            return Ok(None);
        }

        let line_end = buf.iter().position(|b| *b == b'\n').unwrap_or(buf.len());
        let line = String::from_utf8_lossy(&buf[..line_end]).to_string();
        Ok(parse_line(&line))
    }

    async fn rebuild_index(&mut self, mode: RecoveryMode) -> io::Result<()> {
        self.index.clear();

        let data = fs::read(&self.path).await?;
        let mut offset: usize = 0;
        let mut last_good_offset: usize = 0;

        while offset < data.len() {
            let rel = data[offset..].iter().position(|b| *b == b'\n');
            let Some(nl_rel) = rel else {
                return self.handle_corruption(mode, last_good_offset, offset, "incomplete tail line").await;
            };

            let end = offset + nl_rel;
            let line_bytes = &data[offset..end];
            let line = match std::str::from_utf8(line_bytes) {
                Ok(s) => s,
                Err(_) => {
                    return self
                        .handle_corruption(mode, last_good_offset, offset, "non-utf8 line")
                        .await;
                }
            };

            let Some(record) = parse_line(line) else {
                return self
                    .handle_corruption(mode, last_good_offset, offset, "invalid wal format")
                    .await;
            };

            let expected = sha256_hex(&format!("{}\t{}", record.tag, escape_payload(&record.payload)));
            if expected != record.checksum {
                return self
                    .handle_corruption(mode, last_good_offset, offset, "checksum mismatch")
                    .await;
            }

            self.index
                .entry(record.tag.clone())
                .or_default()
                .push(offset as u64);

            offset = end + 1;
            last_good_offset = offset;
        }

        Ok(())
    }

    async fn handle_corruption(
        &self,
        mode: RecoveryMode,
        safe_len: usize,
        corrupted_at: usize,
        reason: &str,
    ) -> io::Result<()> {
        match mode {
            RecoveryMode::Strict => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("wal corrupted at {corrupted_at}: {reason}"),
            )),
            RecoveryMode::TruncateCorruptedTail => {
                eprintln!(
                    "WAL recovery: truncating corrupted tail at offset {} ({})",
                    corrupted_at, reason
                );
                let file = OpenOptions::new().write(true).open(&self.path).await?;
                file.set_len(safe_len as u64).await?;
                Ok(())
            }
        }
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
    input
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
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
