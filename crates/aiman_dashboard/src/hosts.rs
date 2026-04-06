//! Host configuration loading and persistence.

use std::path::Path;

use crate::error::DashboardError;
use crate::types::{HostConfig, HostsFile};

/// Load hosts from JSON store, falling back to TOML config.
///
/// # Errors
///
/// Returns an error if neither file can be read or parsed.
pub async fn load_hosts(
    json_path: &Path,
    toml_path: &Path,
) -> Result<Vec<HostConfig>, DashboardError> {
    // Try JSON store first
    if let Ok(content) = tokio::fs::read_to_string(json_path).await {
        if !content.trim().is_empty() {
            if let Ok(hosts) = serde_json::from_str::<Vec<HostConfig>>(&content) {
                return Ok(hosts);
            }
        }
    }

    // Fallback to TOML config
    if let Ok(content) = tokio::fs::read_to_string(toml_path).await {
        let data: HostsFile = toml::from_str(&content).map_err(|e| {
            DashboardError::ConfigError(format!("invalid TOML: {e}"))
        })?;
        let hosts = data.host.unwrap_or_default();
        if !hosts.is_empty() {
            // Persist to JSON store for future loads
            let _ = persist_hosts(json_path, &hosts).await;
        }
        return Ok(hosts);
    }

    Ok(Vec::new())
}

/// Find a host by ID.
pub fn find_host<'a>(hosts: &'a [HostConfig], id: &str) -> Option<&'a HostConfig> {
    hosts.iter().find(|h| h.id == id)
}

/// Persist hosts to the JSON store.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub async fn persist_hosts(path: &Path, hosts: &[HostConfig]) -> Result<(), DashboardError> {
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(|e| {
            DashboardError::IoError(format!("failed to create directory: {e}"))
        })?;
    }
    let content = serde_json::to_string_pretty(hosts).map_err(|e| {
        DashboardError::Internal(format!("failed to serialize hosts: {e}"))
    })?;
    tokio::fs::write(path, content).await.map_err(|e| {
        DashboardError::IoError(format!("failed to write hosts: {e}"))
    })?;
    Ok(())
}

/// Validate a host configuration.
pub fn validate_host(host: &HostConfig) -> Result<(), DashboardError> {
    if host.id.trim().is_empty() {
        return Err(DashboardError::Validation("id is required".to_string()));
    }
    if host.name.trim().is_empty() {
        return Err(DashboardError::Validation("name is required".to_string()));
    }
    if host.base_url.trim().is_empty() {
        return Err(DashboardError::Validation("base_url is required".to_string()));
    }
    Ok(())
}
