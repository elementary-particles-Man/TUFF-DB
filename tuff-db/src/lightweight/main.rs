use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
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

fn log_event(kind: &str, detail: Option<&str>) {
    let ts = Utc::now().to_rfc3339();
    match detail {
        Some(v) => log_line(&format!("[{}] {}: {}", ts, kind, v)),
        None => log_line(&format!("[{}] {}", ts, kind)),
    }
}

fn is_user_input(tag: &str) -> bool {
    let t = tag.trim().to_ascii_lowercase();
    t == "user" || t == "user-input" || t == "input"
}

fn should_flush_buffer(s: &str) -> bool {
    let t = s.trim_end();
    if t.is_empty() {
        return false;
    }
    if t.len() >= 180 {
        return true;
    }
    matches!(
        t.chars().last(),
        Some('。') | Some('！') | Some('？') | Some('.') | Some('!') | Some('?')
    )
}

async fn flush_ai_buffer(
    tag: &str,
    buffer: &mut String,
    ai_started_ref: &mut bool,
    verifier: &Verifier,
    storage: &Arc<Mutex<WalStorage>>,
    writer_half: &mut tokio::net::tcp::OwnedWriteHalf,
) -> bool {
    if buffer.trim().is_empty() || tag.is_empty() {
        return true;
    }
    let ok = verifier.verify_tag_payload(tag, buffer);
    if !ok {
        log_event("AI_END", Some("mismatch -> disconnect"));
        let _ = writer_half.shutdown().await;
        *ai_started_ref = false;
        return false;
    }

    let mut db = storage.lock().await;
    let _ = db.append(tag, buffer).await;
    buffer.clear();
    true
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
            let mut ai_started = false;
            let mut ai_tag = String::new();
            let mut ai_buffer = String::new();

            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                let (tag, payload) = split_tag_payload(&line);
                let Some(tag) = normalize_tag_key(&tag) else {
                    log_event("AI_END", Some("invalid tag -> disconnect"));
                    let _ = writer_half.shutdown().await;
                    break;
                };

                if is_user_input(&tag) {
                    if ai_started {
                        let ok = flush_ai_buffer(
                            &ai_tag,
                            &mut ai_buffer,
                            &mut ai_started,
                            &verifier,
                            &storage,
                            &mut writer_half,
                        )
                        .await;
                        if !ok {
                            break;
                        }
                        log_event("AI_END", None);
                    }
                    log_event("USER", Some(&payload));
                    continue;
                }

                if !ai_started {
                    ai_started = true;
                    ai_tag = tag.clone();
                    log_event("AI_START", None);
                }

                if ai_tag != tag {
                    let ok = flush_ai_buffer(
                        &ai_tag,
                        &mut ai_buffer,
                        &mut ai_started,
                        &verifier,
                        &storage,
                        &mut writer_half,
                    )
                    .await;
                    if !ok {
                        break;
                    }
                    ai_tag = tag.clone();
                }

                if !ai_buffer.is_empty() {
                    ai_buffer.push(' ');
                }
                ai_buffer.push_str(&payload);

                if should_flush_buffer(&ai_buffer) {
                    let ok = flush_ai_buffer(
                        &ai_tag,
                        &mut ai_buffer,
                        &mut ai_started,
                        &verifier,
                        &storage,
                        &mut writer_half,
                    )
                    .await;
                    if !ok {
                        break;
                    }
                }
            }

            if ai_started {
                let _ = flush_ai_buffer(
                    &ai_tag,
                    &mut ai_buffer,
                    &mut ai_started,
                    &verifier,
                    &storage,
                    &mut writer_half,
                )
                .await;
                log_event("AI_END", None);
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
