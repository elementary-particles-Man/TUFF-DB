use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

mod storage;
mod verifier;

use storage::{RecoveryMode, WalStorage};
use verifier::{normalize_tag_key, MeaningDb, Verifier};

fn log_line(msg: &str) {
    println!("{}", msg);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let wal_path = std::env::var("TUFF_WAL_PATH").unwrap_or_else(|_| "tuff-db-lightweight.wal".to_string());
    let addr = std::env::var("TUFF_LIGHTWEIGHT_ADDR").unwrap_or_else(|_| "127.0.0.1:8788".to_string());
    let recovery_mode = match std::env::var("TUFF_WAL_RECOVERY_MODE") {
        Ok(v) if v.eq_ignore_ascii_case("strict") => RecoveryMode::Strict,
        _ => RecoveryMode::TruncateCorruptedTail,
    };

    let storage = Arc::new(Mutex::new(WalStorage::open(&wal_path, recovery_mode).await?));

    // MeaningDB: env var "TUFF_MEANING_DB" -> "tag=meaning;tag2=meaning2"
    let mut meaning_map = HashMap::new();
    if let Ok(raw) = std::env::var("TUFF_MEANING_DB") {
        for item in raw.split(';') {
            let mut parts = item.splitn(2, '=');
            if let (Some(tag), Some(meaning)) = (parts.next(), parts.next()) {
                if !tag.trim().is_empty() && !meaning.trim().is_empty() {
                    meaning_map.insert(tag.trim().to_string(), meaning.trim().to_string());
                }
            }
        }
    }
    let verifier = Verifier::new(MeaningDb::new(meaning_map));

    let listener = TcpListener::bind(&addr).await?;
    log_line(&format!("TUFF-DB-LIGHTWEIGHT listening on {}", addr));

    loop {
        let (stream, _) = listener.accept().await?;
        let storage = Arc::clone(&storage);
        let verifier = verifier.clone();

        tokio::spawn(async move {
            let (reader_half, mut writer_half) = stream.into_split();
            let mut lines = BufReader::new(reader_half).lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                let (tag, payload) = split_tag_payload(&line);
                let Some(tag) = normalize_tag_key(&tag) else {
                    log_line("INGEST: invalid tag -> disconnect");
                    let _ = writer_half.shutdown().await;
                    break;
                };

                log_line("INGEST: start");
                let ok = verifier.verify_tag_payload(&tag, &payload);
                if !ok {
                    log_line("INGEST: mismatch -> disconnect");
                    let _ = writer_half.shutdown().await;
                    break;
                }

                let mut db = storage.lock().await;
                let _ = db.append(&tag, &payload).await;
                log_line("INGEST: end");
            }
        });
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
