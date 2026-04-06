//! Generic JSONL storage utilities.
//!
//! Provides batched writing and efficient reading of JSONL files.
//! Used by both the agent (for logs) and dashboard (for benchmarks).

use std::collections::VecDeque;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use rev_buf_reader::RevBufReader;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;
use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;

/// Errors that can occur during storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("channel closed")]
    ChannelClosed,

    #[error("task join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

/// Batched JSONL writer backed by an mpsc channel and a background flush task.
///
/// Holds the file open and uses a `BufWriter` for efficient I/O.
/// Batches up to 64 entries per write for throughput.
#[derive(Clone)]
pub struct LogWriter {
    tx: mpsc::Sender<String>,
}

impl LogWriter {
    /// Create a new `LogWriter` that appends JSONL to `path`.
    ///
    /// Spawns a background task that batches writes and flushes periodically.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<String>(4096);
        tokio::spawn(Self::flush_loop(path, rx));
        Self { tx }
    }

    /// Serialize `value` to JSON and send it to the background writer.
    ///
    /// # Errors
    ///
    /// Returns `StorageError::ChannelClosed` if the background writer has stopped.
    pub async fn append<T: Serialize>(&self, value: &T) -> Result<(), StorageError> {
        let line = serde_json::to_string(value)?;
        self.tx
            .send(line)
            .await
            .map_err(|_| StorageError::ChannelClosed)
    }

    async fn flush_loop(path: PathBuf, mut rx: mpsc::Receiver<String>) {
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

/// Read the last `n` lines from a file using reverse reading.
///
/// This is a synchronous function intended for use with `spawn_blocking`.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or read.
pub fn read_last_n_lines(path: &Path, n: usize) -> Result<Vec<String>, StorageError> {
    let file = std::fs::File::open(path)?;
    let reader = RevBufReader::new(file);
    let mut lines: Vec<String> = reader.lines().take(n).collect::<Result<_, _>>()?;
    lines.reverse();
    Ok(lines)
}

/// Read JSONL records with optional timestamp filtering and limit.
///
/// If `since` is `None`, reads efficiently from the end of the file.
/// If `since` is `Some`, scans from the beginning to filter by timestamp.
///
/// Expects records to have an optional `ts` field in RFC 3339 format for filtering.
///
/// # Errors
///
/// Returns an error if the file cannot be read or records cannot be deserialized.
pub async fn read_jsonl<T: DeserializeOwned + Send + 'static>(
    path: &Path,
    since: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<T>, StorageError> {
    let max = limit.unwrap_or(500);

    // Fast path: no timestamp filter — read from end of file.
    if since.is_none() {
        let path = path.to_path_buf();
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
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };

    let since_dt = since.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());

    let reader = tokio::io::BufReader::new(file);
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

/// Best-effort timestamp extraction for since filtering.
fn extract_ts(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value.get("ts")?.as_str().map(ToString::to_string)
}

/// Write data to a temp file then atomically rename to the target path.
///
/// # Errors
///
/// Returns an error if the file cannot be written or renamed.
pub fn atomic_write_sync(path: &Path, data: &[u8]) -> Result<(), StorageError> {
    use std::io::Write;

    let dir = path.parent().unwrap_or(path);
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(data)?;
    tmp.persist(path).map_err(std::io::Error::other)?;
    Ok(())
}
