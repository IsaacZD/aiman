use serde::Serialize;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct ModelArtifact {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub label: String,
    pub library: String,
}

pub fn scan_model_libraries(paths: &[String]) -> Vec<ModelArtifact> {
    let mut artifacts = Vec::new();
    for raw_path in paths {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        scan_library(&path, &mut artifacts);
    }
    artifacts.sort_by(|a, b| a.label.cmp(&b.label));
    artifacts
}

fn scan_library(path: &Path, artifacts: &mut Vec<ModelArtifact>) {
    let library_label = path.display().to_string();
    let library_hint = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&library_label)
        .to_string();
    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        if name.starts_with("models--") {
            let model_id = name
                .trim_start_matches("models--")
                .replace("--", "/");
            scan_model_dir(path, &model_id, &library_label, &library_hint, artifacts);
            return;
        }
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(
                library = %library_label,
                error = %err,
                "failed to read model library"
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let name = match entry_path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name,
            None => continue,
        };
        if !name.starts_with("models--") {
            continue;
        }
        let model_id = name
            .trim_start_matches("models--")
            .replace("--", "/");
        scan_model_dir(&entry_path, &model_id, &library_label, &library_hint, artifacts);
    }
}

fn scan_model_dir(
    path: &Path,
    model_id: &str,
    library_label: &str,
    library_hint: &str,
    artifacts: &mut Vec<ModelArtifact>,
) {
    let snapshots_dir = path.join("snapshots");
    let entries = match fs::read_dir(&snapshots_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let snapshot_path = entry.path();
        if !snapshot_path.is_dir() {
            continue;
        }
        let snapshot_name = snapshot_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("snapshot");
        artifacts.push(ModelArtifact {
            id: model_id.to_string(),
            kind: "snapshot".to_string(),
            path: snapshot_path.display().to_string(),
            label: format!("{model_id} • snapshot {snapshot_name} @ {library_hint}"),
            library: library_label.to_string(),
        });

        collect_gguf_files(
            &snapshot_path,
            model_id,
            library_label,
            library_hint,
            artifacts,
            0,
        );
    }
}

fn collect_gguf_files(
    dir: &Path,
    model_id: &str,
    library_label: &str,
    library_hint: &str,
    artifacts: &mut Vec<ModelArtifact>,
    depth: usize,
) {
    if depth > 4 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_gguf_files(
                &path,
                model_id,
                library_label,
                library_hint,
                artifacts,
                depth + 1,
            );
            continue;
        }
        if path
            .extension()
            .and_then(OsStr::to_str)
            .map(|ext| ext.eq_ignore_ascii_case("gguf"))
            .unwrap_or(false)
        {
            let filename = path.file_name().and_then(|name| name.to_str()).unwrap_or("model.gguf");
            artifacts.push(ModelArtifact {
                id: model_id.to_string(),
                kind: "gguf".to_string(),
                path: path.display().to_string(),
                label: format!("{model_id} • {filename} @ {library_hint}"),
                library: library_label.to_string(),
            });
        }
    }
}
