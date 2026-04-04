use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use aiman_shared::{LogEntry, LogSession};
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncBufReadExt,
    io::BufReader,
    sync::Mutex,
};

// Append a JSONL line with a simple mutex to avoid interleaving.
pub async fn append_jsonl<T: Serialize>(path: &PathBuf, lock: &Arc<Mutex<()>>, value: &T) {
    let _guard = lock.lock().await;
    if let Ok(line) = serde_json::to_string(value) {
        if let Ok(mut file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
        {
            use tokio::io::AsyncWriteExt;
            let _ = file.write_all(line.as_bytes()).await;
            let _ = file.write_all(b"\n").await;
        }
    }
}

pub async fn append_session(path: &PathBuf, lock: &Arc<Mutex<()>>, session: &LogSession) {
    append_jsonl(path, lock, session).await;
}

// Read log JSONL with optional since + session filtering.
pub async fn read_log_entries(
    path: &PathBuf,
    since: Option<&str>,
    limit: Option<usize>,
    session_id: Option<&str>,
) -> anyhow::Result<Vec<LogEntry>> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return Ok(Vec::new()),
    };

    let since_dt = since.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut entries: VecDeque<LogEntry> = VecDeque::new();
    let max = limit.unwrap_or(500);

    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
            if let Some(since_dt) = since_dt {
                if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&entry.ts) {
                    if parsed < since_dt {
                        continue;
                    }
                }
            }
            if let Some(session_id) = session_id {
                if entry.session_id != session_id {
                    continue;
                }
            }
            if entries.len() >= max {
                entries.pop_front();
            }
            entries.push_back(entry);
        }
    }

    Ok(entries.into_iter().collect())
}

// Read log sessions and collapse start/stop records into a single entry per session.
pub async fn read_log_sessions(
    path: &PathBuf,
    limit: Option<usize>,
) -> anyhow::Result<Vec<LogSession>> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return Ok(Vec::new()),
    };

    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut sessions: HashMap<String, LogSession> = HashMap::new();

    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(entry) = serde_json::from_str::<LogSession>(&line) {
            let session = sessions.entry(entry.id.clone()).or_insert(LogSession {
                id: entry.id.clone(),
                started_at: entry.started_at.clone(),
                stopped_at: None,
            });
            if entry.started_at < session.started_at {
                session.started_at = entry.started_at.clone();
            }
            if entry.stopped_at.is_some() {
                session.stopped_at = entry.stopped_at.clone();
            }
        }
    }

    let mut values: Vec<_> = sessions.into_values().collect();
    values.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    if let Some(limit) = limit {
        values.truncate(limit);
    }

    Ok(values)
}

// Read JSONL with optional since + limit filtering.
pub async fn read_jsonl<T: for<'de> Deserialize<'de>>(
    path: &PathBuf,
    since: Option<&str>,
    limit: Option<usize>,
) -> anyhow::Result<Vec<T>> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return Ok(Vec::new()),
    };

    let since_dt = since.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());

    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut entries: VecDeque<T> = VecDeque::new();
    let max = limit.unwrap_or(500);

    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(entry) = serde_json::from_str::<T>(&line) {
            if let Some(since_dt) = since_dt {
                if let Some(ts) = extract_ts(&line) {
                    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&ts) {
                        if parsed < since_dt {
                            continue;
                        }
                    }
                }
            }

            if entries.len() >= max {
                entries.pop_front();
            }
            entries.push_back(entry);
        }
    }

    Ok(entries.into_iter().collect())
}

// Best-effort timestamp extraction for since filtering.
fn extract_ts(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value.get("ts")?.as_str().map(|s| s.to_string())
}
