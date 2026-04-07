mod error;
mod handle;
mod store;

pub use error::{map_supervisor_error, SupervisorError};
pub use handle::EngineHandle;
pub use store::{read_jsonl, read_log_entries, read_log_sessions};

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use aiman_shared::{ContainerImage, EngineConfig, EngineInstance, EngineStatus, EngineType, ImageStatus};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};

use crate::hardware::HardwareInfo;

use self::store::LogWriter;

#[derive(Clone)]
// Supervisor holds engine handles and mediates lifecycle control.
pub struct Supervisor {
    config_path: PathBuf,
    data_dir: PathBuf,
    configs: Arc<RwLock<HashMap<String, EngineConfig>>>,
    handles: Arc<RwLock<HashMap<String, Arc<EngineHandle>>>>,
    images: Arc<RwLock<HashMap<String, ContainerImage>>>,
    images_path: PathBuf,
    benchmark_path: PathBuf,
    benchmark_writer: LogWriter,
    // Broadcast channels for reactive push to SSE clients.
    status_tx: broadcast::Sender<EngineInstance>,
    hardware_tx: broadcast::Sender<HardwareInfo>,
    image_status_tx: broadcast::Sender<ContainerImage>,
}

impl Supervisor {
    pub async fn from_store(
        config_path: PathBuf,
        data_dir: PathBuf,
        seed_path: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        tracing::info!(
            config_path = %config_path.display(),
            data_dir = %data_dir.display(),
            seed_path = seed_path.as_ref().map(|path| path.display().to_string()),
            "loading engine config store"
        );
        // Ensure persistence directories exist.
        tokio::fs::create_dir_all(data_dir.join("logs")).await?;
        tokio::fs::create_dir_all(data_dir.join("status")).await?;
        if let Some(parent) = config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let configs_vec = load_config_store(&config_path, seed_path.as_ref()).await?;
        tracing::info!(count = configs_vec.len(), "loaded engine configs");
        let images_path = data_dir.join("container-images.json");
        let images_vec = load_image_store(&images_path).await?;
        tracing::info!(count = images_vec.len(), "loaded container images");
        let mut configs = HashMap::new();
        let mut handles = HashMap::new();
        let images = Arc::new(RwLock::new(images_to_map(&images_vec)));
        let benchmark_path = data_dir.join("benchmarks.jsonl");

        // Broadcast channels: capacity 256 for status events (one per transition),
        // 16 for hardware (periodic, low frequency), 64 for image status updates.
        let (status_tx, _) = broadcast::channel::<EngineInstance>(256);
        let (hardware_tx, _) = broadcast::channel::<HardwareInfo>(16);
        let (image_status_tx, _) = broadcast::channel::<ContainerImage>(64);

        for config in configs_vec {
            let id = config.id.clone();
            tracing::debug!(engine_id = %id, "registering engine config");
            configs.insert(id.clone(), config.clone());
            handles.insert(
                id.clone(),
                Arc::new(EngineHandle::new(
                    config,
                    images.clone(),
                    data_dir.join("logs").join(format!("{id}.jsonl")),
                    data_dir.join("logs").join(format!("{id}-sessions.jsonl")),
                    data_dir.join("status").join(format!("{id}.jsonl")),
                    status_tx.clone(),
                )),
            );
        }

        Ok(Self {
            config_path,
            data_dir,
            configs: Arc::new(RwLock::new(configs)),
            handles: Arc::new(RwLock::new(handles)),
            images,
            images_path,
            benchmark_writer: LogWriter::new(benchmark_path.clone()),
            benchmark_path,
            status_tx,
            hardware_tx,
            image_status_tx,
        })
    }

    pub async fn list_instances(&self) -> Vec<EngineInstance> {
        let handles = self.handles.read().await;
        let mut instances = Vec::with_capacity(handles.len());
        for handle in handles.values() {
            instances.push(handle.instance.read().await.clone());
        }
        instances
    }

    pub async fn get_instance(&self, id: &str) -> Option<EngineInstance> {
        let handles = self.handles.read().await;
        let handle = handles.get(id)?.clone();
        let instance = handle.instance.read().await.clone();
        Some(instance)
    }

    pub async fn get_handle(&self, id: &str) -> Option<Arc<EngineHandle>> {
        let handles = self.handles.read().await;
        handles.get(id).cloned()
    }

    pub async fn list_configs(&self) -> Vec<EngineConfig> {
        let configs = self.configs.read().await;
        let mut values: Vec<_> = configs.values().cloned().collect();
        values.sort_by(|a, b| a.id.cmp(&b.id));
        values
    }

    pub async fn get_config(&self, id: &str) -> Option<EngineConfig> {
        let configs = self.configs.read().await;
        configs.get(id).cloned()
    }

    pub async fn list_images(&self) -> Vec<ContainerImage> {
        let images = self.images.read().await;
        let mut values: Vec<_> = images.values().cloned().collect();
        values.sort_by(|a, b| a.id.cmp(&b.id));
        values
    }

    pub async fn get_image(&self, id: &str) -> Option<ContainerImage> {
        let images = self.images.read().await;
        images.get(id).cloned()
    }

    pub async fn add_image(
        &self,
        mut image: ContainerImage,
    ) -> Result<ContainerImage, SupervisorError> {
        validate_image(&image)?;
        image.status = ImageStatus::NotReady;
        let mut images = self.images.write().await;
        if images.contains_key(&image.id) {
            return Err(SupervisorError::ImageExists);
        }
        let id = image.id.clone();
        images.insert(id.clone(), image.clone());
        persist_image_store(&self.images_path, &images).await;
        tracing::info!(image_id = %id, "added container image");
        Ok(image)
    }

    pub async fn update_image(
        &self,
        id: &str,
        mut image: ContainerImage,
    ) -> Result<ContainerImage, SupervisorError> {
        validate_image(&image)?;
        if id != image.id {
            return Err(SupervisorError::ImageInvalid(
                "image id mismatch".to_string(),
            ));
        }
        let mut images = self.images.write().await;
        let existing = images.get(id).ok_or(SupervisorError::ImageNotFound)?;

        // Reset status to NotReady if the image source changed.
        let image_ref_changed = existing.image != image.image;
        let dockerfile_changed = existing.build.as_ref().and_then(|b| b.dockerfile_content.as_deref())
            != image.build.as_ref().and_then(|b| b.dockerfile_content.as_deref());
        if image_ref_changed || dockerfile_changed {
            image.status = ImageStatus::NotReady;
        } else {
            // Preserve existing status — don't let the client overwrite it.
            image.status = existing.status.clone();
        }

        images.insert(id.to_string(), image.clone());
        persist_image_store(&self.images_path, &images).await;
        let _ = self.image_status_tx.send(image.clone());
        tracing::info!(image_id = %id, "updated container image");
        Ok(image)
    }

    pub async fn remove_image(&self, id: &str) -> Result<(), SupervisorError> {
        let configs = self.configs.read().await;
        if configs.values().any(|config| {
            config.engine_type == EngineType::Container
                && config
                    .container
                    .as_ref()
                    .map(|c| c.image_id == id)
                    .unwrap_or(false)
        }) {
            return Err(SupervisorError::ImageInUse);
        }
        drop(configs);
        let mut images = self.images.write().await;
        if images.remove(id).is_none() {
            return Err(SupervisorError::ImageNotFound);
        }
        persist_image_store(&self.images_path, &images).await;
        tracing::info!(image_id = %id, "removed container image");
        Ok(())
    }

    /// Prepare (pull or build) a container image asynchronously.
    /// Returns the image with status set to `Preparing`.
    /// The actual work runs in a background task; status updates are broadcast via SSE.
    pub async fn prepare_image(&self, id: &str) -> Result<ContainerImage, SupervisorError> {
        let mut images = self.images.write().await;
        let image = images.get_mut(id).ok_or(SupervisorError::ImageNotFound)?;
        if image.status == ImageStatus::Preparing {
            return Err(SupervisorError::ImagePreparing);
        }
        image.status = ImageStatus::Preparing;
        let image_snapshot = image.clone();
        persist_image_store(&self.images_path, &images).await;
        let _ = self.image_status_tx.send(image_snapshot.clone());
        drop(images);

        // Spawn background task to do the actual pull/build.
        let images_ref = self.images.clone();
        let images_path = self.images_path.clone();
        let tx = self.image_status_tx.clone();
        let image_clone = image_snapshot.clone();
        tokio::spawn(async move {
            let result = handle::prepare_image_task(&image_clone).await;
            let mut images = images_ref.write().await;
            if let Some(img) = images.get_mut(&image_clone.id) {
                match result {
                    Ok(()) => {
                        img.status = ImageStatus::Ready;
                        tracing::info!(image_id = %image_clone.id, "image prepared successfully");
                    }
                    Err(err) => {
                        img.status = ImageStatus::Failed;
                        tracing::error!(image_id = %image_clone.id, error = %err, "image preparation failed");
                    }
                }
                let updated = img.clone();
                persist_image_store(&images_path, &images).await;
                let _ = tx.send(updated);
            }
        });

        Ok(image_snapshot)
    }

    pub async fn add_config(&self, config: EngineConfig) -> Result<EngineConfig, SupervisorError> {
        validate_config(&config)?;
        if matches!(config.engine_type, EngineType::Container) {
            let images = self.images.read().await;
            validate_container_config(&config, &images)?;
        }
        let mut configs = self.configs.write().await;
        if configs.contains_key(&config.id) {
            return Err(SupervisorError::ConfigExists);
        }
        let mut handles = self.handles.write().await;

        let id = config.id.clone();
        configs.insert(id.clone(), config.clone());
        handles.insert(
            id.clone(),
            Arc::new(EngineHandle::new(
                config.clone(),
                self.images.clone(),
                self.data_dir.join("logs").join(format!("{id}.jsonl")),
                self.data_dir.join("logs").join(format!("{id}-sessions.jsonl")),
                self.data_dir.join("status").join(format!("{id}.jsonl")),
                self.status_tx.clone(),
            )),
        );

        persist_config_store(&self.config_path, &configs).await;
        tracing::info!(engine_id = %id, "added engine config");
        Ok(config)
    }

    pub async fn update_config(
        &self,
        id: &str,
        config: EngineConfig,
    ) -> Result<EngineConfig, SupervisorError> {
        validate_config(&config)?;
        if matches!(config.engine_type, EngineType::Container) {
            let images = self.images.read().await;
            validate_container_config(&config, &images)?;
        }
        if id != config.id {
            return Err(SupervisorError::ConfigInvalid(
                "config id mismatch".to_string(),
            ));
        }

        let mut configs = self.configs.write().await;
        let mut handles = self.handles.write().await;

        let handle = handles.get(id).cloned().ok_or(SupervisorError::NotFound)?;
        if handle_is_running(&handle).await {
            return Err(SupervisorError::ConfigInUse);
        }

        configs.insert(id.to_string(), config.clone());
        handles.insert(
            id.to_string(),
            Arc::new(EngineHandle::new(
                config.clone(),
                self.images.clone(),
                self.data_dir.join("logs").join(format!("{id}.jsonl")),
                self.data_dir.join("logs").join(format!("{id}-sessions.jsonl")),
                self.data_dir.join("status").join(format!("{id}.jsonl")),
                self.status_tx.clone(),
            )),
        );

        persist_config_store(&self.config_path, &configs).await;
        tracing::info!(engine_id = %id, "updated engine config");
        Ok(config)
    }

    pub async fn remove_config(&self, id: &str) -> Result<(), SupervisorError> {
        let mut configs = self.configs.write().await;
        let mut handles = self.handles.write().await;
        let handle = handles.get(id).cloned().ok_or(SupervisorError::NotFound)?;
        if handle_is_running(&handle).await {
            return Err(SupervisorError::ConfigInUse);
        }
        configs.remove(id);
        handles.remove(id);
        persist_config_store(&self.config_path, &configs).await;
        tracing::info!(engine_id = %id, "removed engine config");
        Ok(())
    }

    pub async fn start(&self, id: &str) -> Result<EngineInstance, SupervisorError> {
        tracing::info!(engine_id = %id, "start requested");
        let handle = self.get_handle(id).await.ok_or(SupervisorError::NotFound)?;
        handle.start().await?;
        let instance = handle.instance.read().await.clone();
        Ok(instance)
    }

    pub async fn stop(&self, id: &str) -> Result<EngineInstance, SupervisorError> {
        tracing::info!(engine_id = %id, "stop requested");
        let handle = self.get_handle(id).await.ok_or(SupervisorError::NotFound)?;
        handle.stop().await?;
        let instance = handle.instance.read().await.clone();
        Ok(instance)
    }

    /// Remove any container image labeled `managed-by=aiman` whose tag is no longer
    /// referenced by any current aiman image config.  Returns the list of removed tags.
    pub async fn prune_images(&self) -> Result<Vec<String>, SupervisorError> {
        use tokio::process::Command;

        // Collect all tags currently referenced by aiman image configs.
        let images = self.images.read().await;
        let live_tags: HashSet<String> = images.values().map(|img| img.image.clone()).collect();
        drop(images);

        // Ask Podman for every image we previously labeled.
        let output = Command::new("podman")
            .args(["images", "--filter", "label=managed-by=aiman", "--format", "json"])
            .output()
            .await
            .map_err(|e| SupervisorError::ContainerApi(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SupervisorError::ContainerApi(format!("podman images failed: {}", stderr.trim())));
        }

        let daemon_images: Vec<serde_json::Value> =
            serde_json::from_slice(&output.stdout).unwrap_or_default();

        let mut removed = Vec::new();
        for img in daemon_images {
            let tags = img.get("Names")
                .or_else(|| img.get("names"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for tag_val in &tags {
                let Some(tag) = tag_val.as_str() else { continue };
                if !live_tags.contains(tag) {
                    let result = Command::new("podman")
                        .args(["rmi", tag])
                        .output()
                        .await;
                    match result {
                        Ok(out) if out.status.success() => {
                            tracing::info!(tag = %tag, "pruned orphaned aiman-managed image");
                            removed.push(tag.to_string());
                        }
                        Ok(out) => {
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            tracing::warn!(tag = %tag, error = %stderr.trim(), "failed to prune image");
                        }
                        Err(err) => {
                            tracing::warn!(tag = %tag, error = %err, "failed to prune image");
                        }
                    }
                }
            }
        }
        Ok(removed)
    }

    pub fn subscribe_status(&self) -> broadcast::Receiver<EngineInstance> {
        self.status_tx.subscribe()
    }

    pub fn subscribe_hardware(&self) -> broadcast::Receiver<HardwareInfo> {
        self.hardware_tx.subscribe()
    }

    pub fn subscribe_image_status(&self) -> broadcast::Receiver<ContainerImage> {
        self.image_status_tx.subscribe()
    }

    /// Returns a clone of the hardware sender so the polling task in main can push updates.
    pub fn hardware_tx(&self) -> broadcast::Sender<HardwareInfo> {
        self.hardware_tx.clone()
    }

    pub fn benchmark_path(&self) -> &PathBuf {
        &self.benchmark_path
    }

    pub async fn append_benchmark<T: Serialize>(&self, record: &T) {
        // Ignore errors - LogWriter logs internally on failure
        let _ = self.benchmark_writer.append(record).await;
    }
}

#[derive(Deserialize)]
struct ConfigFile {
    engine: Vec<EngineConfig>,
}

async fn load_config_store(
    path: &PathBuf,
    seed_path: Option<&PathBuf>,
) -> anyhow::Result<Vec<EngineConfig>> {
    if let Ok(raw) = tokio::fs::read_to_string(path).await {
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        let configs: Vec<EngineConfig> = serde_json::from_str(&raw)?;
        return Ok(configs);
    }

    if let Some(seed_path) = seed_path {
        if let Ok(raw) = tokio::fs::read_to_string(seed_path).await {
            let parsed: ConfigFile = toml::from_str(&raw)?;
            let configs = parsed.engine;
            persist_config_store(path, &configs_to_map(&configs)).await;
            return Ok(configs);
        }
    }

    Ok(Vec::new())
}

async fn load_image_store(path: &PathBuf) -> anyhow::Result<Vec<ContainerImage>> {
    if let Ok(raw) = tokio::fs::read_to_string(path).await {
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        let images: Vec<ContainerImage> = serde_json::from_str(&raw)?;
        return Ok(images);
    }
    Ok(Vec::new())
}

fn configs_to_map(configs: &[EngineConfig]) -> HashMap<String, EngineConfig> {
    configs
        .iter()
        .cloned()
        .map(|config| (config.id.clone(), config))
        .collect()
}

fn images_to_map(images: &[ContainerImage]) -> HashMap<String, ContainerImage> {
    images
        .iter()
        .cloned()
        .map(|image| (image.id.clone(), image))
        .collect()
}

async fn persist_config_store(path: &PathBuf, configs: &HashMap<String, EngineConfig>) {
    let mut values: Vec<_> = configs.values().cloned().collect();
    values.sort_by(|a, b| a.id.cmp(&b.id));
    if let Ok(serialized) = serde_json::to_string_pretty(&values) {
        let path = path.clone();
        match tokio::task::spawn_blocking(move || {
            store::atomic_write_json(&path, serialized.as_bytes())
        })
        .await
        {
            Err(err) => tracing::error!(error = %err, "failed to persist config store"),
            Ok(Err(err)) => tracing::error!(error = %err, "failed to persist config store"),
            Ok(Ok(())) => {}
        }
    }
}

async fn persist_image_store(path: &PathBuf, images: &HashMap<String, ContainerImage>) {
    let mut values: Vec<_> = images.values().cloned().collect();
    values.sort_by(|a, b| a.id.cmp(&b.id));
    if let Ok(serialized) = serde_json::to_string_pretty(&values) {
        let path = path.clone();
        match tokio::task::spawn_blocking(move || {
            store::atomic_write_json(&path, serialized.as_bytes())
        })
        .await
        {
            Err(err) => tracing::error!(error = %err, "failed to persist image store"),
            Ok(Err(err)) => tracing::error!(error = %err, "failed to persist image store"),
            Ok(Ok(())) => {}
        }
    }
}

fn validate_config(config: &EngineConfig) -> Result<(), SupervisorError> {
    if config.id.trim().is_empty() {
        return Err(SupervisorError::ConfigInvalid("id is required".to_string()));
    }
    if config.name.trim().is_empty() {
        return Err(SupervisorError::ConfigInvalid("name is required".to_string()));
    }
    if matches!(config.engine_type, EngineType::Container) {
        let container = config
            .container
            .as_ref()
            .ok_or_else(|| SupervisorError::ConfigInvalid("container config is required".to_string()))?;
        if container.image_id.trim().is_empty() {
            return Err(SupervisorError::ConfigInvalid(
                "container image id is required".to_string(),
            ));
        }
    } else if config.command.trim().is_empty() {
        return Err(SupervisorError::ConfigInvalid(
            "command is required".to_string(),
        ));
    }
    Ok(())
}

fn validate_container_config(
    config: &EngineConfig,
    images: &HashMap<String, ContainerImage>,
) -> Result<(), SupervisorError> {
    let container = config
        .container
        .as_ref()
        .ok_or_else(|| SupervisorError::ConfigInvalid("container config is required".to_string()))?;
    if !images.contains_key(&container.image_id) {
        return Err(SupervisorError::ImageNotFound);
    }
    Ok(())
}

fn validate_image(image: &ContainerImage) -> Result<(), SupervisorError> {
    if image.id.trim().is_empty() {
        return Err(SupervisorError::ImageInvalid("id is required".to_string()));
    }
    if image.name.trim().is_empty() {
        return Err(SupervisorError::ImageInvalid("name is required".to_string()));
    }

    let has_image_ref = !image.image.trim().is_empty();
    let has_dockerfile = image
        .build
        .as_ref()
        .and_then(|b| b.dockerfile_content.as_deref())
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);

    if has_image_ref && has_dockerfile {
        return Err(SupervisorError::ImageInvalid(
            "image reference and Dockerfile are mutually exclusive".to_string(),
        ));
    }
    if !has_image_ref && !has_dockerfile {
        return Err(SupervisorError::ImageInvalid(
            "either an image reference or Dockerfile content is required".to_string(),
        ));
    }

    Ok(())
}

async fn handle_is_running(handle: &EngineHandle) -> bool {
    matches!(
        handle.instance.read().await.status,
        EngineStatus::Running | EngineStatus::Starting
    )
}
