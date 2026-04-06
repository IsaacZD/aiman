//! Log storage utilities for the agent.
//!
//! Provides log-specific reading functions that build on the shared
//! JSONL utilities.

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use aiman_shared::storage::read_last_n_lines;
use aiman_shared::{LogEntry, LogSession};
use tokio::io::{AsyncBufReadExt, BufReader};

// Re-export from shared for backwards compatibility
pub use aiman_shared::storage::{atomic_write_sync as atomic_write_json, read_jsonl, LogWriter};

/// Read log entries with optional timestamp, session, and limit filtering.
///
/// If `since` is `None`, reads efficiently from the end of the file.
/// If `since` is `Some`, scans from the beginning to filter by timestamp.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub async fn read_log_entries(
    path: &Path,
    since: Option<&str>,
    limit: Option<usize>,
    session_id: Option<&str>,
) -> anyhow::Result<Vec<LogEntry>> {
    let max = limit.unwrap_or(500);

    // Fast path: no timestamp filter — read from end of file.
    if since.is_none() {
        let path = path.to_path_buf();
        let session_filter = session_id.map(|s| s.to_string());
        // Over-read to account for filtered-out lines, then truncate.
        let read_count = max * 4;
        let lines = tokio::task::spawn_blocking(move || read_last_n_lines(&path, read_count))
            .await?
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut entries: Vec<LogEntry> = Vec::with_capacity(max);
        for line in lines {
            if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                if let Some(ref sid) = session_filter {
                    if entry.session_id != *sid {
                        continue;
                    }
                }
                entries.push(entry);
            }
        }
        // Keep only the last `max` entries (they're already in chronological order).
        if entries.len() > max {
            entries.drain(..entries.len() - max);
        }
        return Ok(entries);
    }

    // Slow path: timestamp filter requires scanning from the beginning.
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return Ok(Vec::new()),
    };

    let since_dt = since.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut entries: VecDeque<LogEntry> = VecDeque::new();

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

/// Read log sessions and collapse start/stop records into a single entry per session.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub async fn read_log_sessions(path: &Path, limit: Option<usize>) -> anyhow::Result<Vec<LogSession>> {
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
                session.started_at.clone_from(&entry.started_at);
            }
            if entry.stopped_at.is_some() {
                session.stopped_at.clone_from(&entry.stopped_at);
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
