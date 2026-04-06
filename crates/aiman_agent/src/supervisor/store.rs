use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, Write};
use std::path::PathBuf;

use aiman_shared::{LogEntry, LogSession};
use rev_buf_reader::RevBufReader;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};

/// Batched JSONL writer backed by an mpsc channel and a background flush task.
/// Holds the file open and uses a BufWriter for efficient I/O.
#[derive(Clone)]
pub struct LogWriter {
    tx: tokio::sync::mpsc::Sender<String>,
}

impl LogWriter {
    /// Create a new LogWriter that appends JSONL to `path`.
    /// Spawns a background task that batches writes and flushes periodically.
    pub fn new(path: PathBuf) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(4096);
        tokio::spawn(Self::flush_loop(path, rx));
        Self { tx }
    }

    /// Serialize `value` to JSON and send it to the background writer.
    pub async fn append<T: Serialize>(&self, value: &T) {
        if let Ok(line) = serde_json::to_string(value) {
            if self.tx.send(line).await.is_err() {
                tracing::warn!("log writer channel closed");
            }
        }
    }

    async fn flush_loop(path: PathBuf, mut rx: tokio::sync::mpsc::Receiver<String>) {
        use tokio::io::AsyncWriteExt;

        let file = match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
        {
            Ok(f) => f,
            Err(err) => {
                tracing::error!(path = %path.display(), error = %err, "failed to open log file");
                return;
            }
        };

        let mut writer = tokio::io::BufWriter::new(file);
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut buf = String::new();

        loop {
            tokio::select! {
                maybe = rx.recv() => {
                    let Some(line) = maybe else {
                        // Channel closed — flush and exit.
                        if !buf.is_empty() {
                            if let Err(err) = writer.write_all(buf.as_bytes()).await {
                                tracing::warn!(path = %path.display(), error = %err, "JSONL write error");
                            }
                            let _ = writer.flush().await;
                        }
                        return;
                    };
                    buf.push_str(&line);
                    buf.push('\n');

                    // Drain up to 63 more entries if available (batch of 64).
                    for _ in 0..63 {
                        match rx.try_recv() {
                            Ok(line) => {
                                buf.push_str(&line);
                                buf.push('\n');
                            }
                            Err(_) => break,
                        }
                    }

                    if let Err(err) = writer.write_all(buf.as_bytes()).await {
                        tracing::warn!(path = %path.display(), error = %err, "JSONL write error");
                    }
                    buf.clear();
                    let _ = writer.flush().await;
                }
                _ = interval.tick() => {
                    // Periodic flush in case BufWriter has buffered data.
                    let _ = writer.flush().await;
                }
            }
        }
    }
}

/// Read the last `n` lines from a file using reverse reading (sync, use in spawn_blocking).
fn read_last_n_lines(path: &PathBuf, n: usize) -> anyhow::Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let reader = RevBufReader::new(file);
    let mut lines: Vec<String> = reader
        .lines()
        .take(n)
        .collect::<Result<_, _>>()?;
    lines.reverse();
    Ok(lines)
}

// Read log JSONL with optional since + session filtering.
pub async fn read_log_entries(
    path: &PathBuf,
    since: Option<&str>,
    limit: Option<usize>,
    session_id: Option<&str>,
) -> anyhow::Result<Vec<LogEntry>> {
    let max = limit.unwrap_or(500);

    // Fast path: no timestamp filter — read from end of file.
    if since.is_none() {
        let path = path.clone();
        let session_filter = session_id.map(|s| s.to_string());
        // Over-read to account for filtered-out lines, then truncate.
        let read_count = max * 4;
        let lines = tokio::task::spawn_blocking(move || read_last_n_lines(&path, read_count)).await??;

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
pub async fn read_jsonl<T: for<'de> Deserialize<'de> + Send + 'static>(
    path: &PathBuf,
    since: Option<&str>,
    limit: Option<usize>,
) -> anyhow::Result<Vec<T>> {
    let max = limit.unwrap_or(500);

    // Fast path: no timestamp filter — read from end of file.
    if since.is_none() {
        let path = path.clone();
        let lines = tokio::task::spawn_blocking(move || read_last_n_lines(&path, max)).await??;

        let entries: Vec<T> = lines
            .into_iter()
            .filter_map(|line| serde_json::from_str::<T>(&line).ok())
            .collect();
        return Ok(entries);
    }

    // Slow path: timestamp filter requires full scan.
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return Ok(Vec::new()),
    };

    let since_dt = since.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());

    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut entries: VecDeque<T> = VecDeque::new();

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

/// Write data to a temp file then atomically rename to the target path.
pub fn atomic_write_json(path: &PathBuf, data: &[u8]) -> anyhow::Result<()> {
    let dir = path.parent().unwrap_or(path);
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(data)?;
    tmp.persist(path)?;
    Ok(())
}
