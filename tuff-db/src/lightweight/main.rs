use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;

mod storage;
mod verifier;

use storage::WalStorage;
use verifier::{MeaningDb, Verifier};

fn log_line(msg: &str) {
    println!("{}", msg);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let wal_path = std::env::var("TUFF_WAL_PATH").unwrap_or_else(|_| "tuff-db-lightweight.wal".to_string());
    let addr = std::env::var("TUFF_LIGHTWEIGHT_ADDR").unwrap_or_else(|_| "127.0.0.1:8788".to_string());

    let storage = Arc::new(Mutex::new(WalStorage::open(&wal_path)?));

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
        tokio::task::spawn_blocking(move || {
            let std_stream = match stream.into_std() {
                Ok(s) => s,
                Err(_) => return,
            };
            let read_stream = match std_stream.try_clone() {
                Ok(s) => s,
                Err(_) => return,
            };
            let reader = BufReader::new(read_stream);
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let (tag, payload) = split_tag_payload(&line);
                log_line("INGEST: start");
                let ok = verifier.verify_or_disconnect(&tag, &payload, &std_stream);
                if !ok {
                    log_line("INGEST: mismatch -> disconnect");
                    break;
                }
                if let Ok(mut db) = storage.lock() {
                    let _ = db.append(&tag, &payload);
                }
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
