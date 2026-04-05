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

use aiman_shared::{ContainerImage, EngineConfig, EngineInstance, EngineStatus, EngineType};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::hardware::HardwareInfo;

use self::store::append_jsonl;

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
    benchmark_write_lock: Arc<Mutex<()>>,
    // Broadcast channels for reactive push to SSE clients.
    status_tx: broadcast::Sender<EngineInstance>,
    hardware_tx: broadcast::Sender<HardwareInfo>,
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
        // 16 for hardware (periodic, low frequency).
        let (status_tx, _) = broadcast::channel::<EngineInstance>(256);
        let (hardware_tx, _) = broadcast::channel::<HardwareInfo>(16);

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
            benchmark_path,
            benchmark_write_lock: Arc::new(Mutex::new(())),
            status_tx,
            hardware_tx,
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
        image: ContainerImage,
    ) -> Result<ContainerImage, SupervisorError> {
        validate_image(&image)?;
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
        image: ContainerImage,
    ) -> Result<ContainerImage, SupervisorError> {
        validate_image(&image)?;
        if id != image.id {
            return Err(SupervisorError::ImageInvalid(
                "image id mismatch".to_string(),
            ));
        }
        let mut images = self.images.write().await;
        if !images.contains_key(id) {
            return Err(SupervisorError::ImageNotFound);
        }
        images.insert(id.to_string(), image.clone());
        persist_image_store(&self.images_path, &images).await;
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

    /// Returns a clone of the hardware sender so the polling task in main can push updates.
    pub fn hardware_tx(&self) -> broadcast::Sender<HardwareInfo> {
        self.hardware_tx.clone()
    }

    pub fn benchmark_path(&self) -> &PathBuf {
        &self.benchmark_path
    }

    pub async fn append_benchmark<T: Serialize>(&self, record: &T) {
        append_jsonl(&self.benchmark_path, &self.benchmark_write_lock, record).await;
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
        let _ = tokio::fs::write(path, serialized).await;
    }
}

async fn persist_image_store(path: &PathBuf, images: &HashMap<String, ContainerImage>) {
    let mut values: Vec<_> = images.values().cloned().collect();
    values.sort_by(|a, b| a.id.cmp(&b.id));
    if let Ok(serialized) = serde_json::to_string_pretty(&values) {
        let _ = tokio::fs::write(path, serialized).await;
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
    if image.image.trim().is_empty() {
        return Err(SupervisorError::ImageInvalid("image is required".to_string()));
    }
    if let Some(build) = &image.build {
        if build
            .dockerfile_content
            .as_ref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(SupervisorError::ImageInvalid(
                "dockerfile_content is required when build is enabled".to_string(),
            ));
        }
    }
    Ok(())
}

async fn handle_is_running(handle: &EngineHandle) -> bool {
    matches!(
        handle.instance.read().await.status,
        EngineStatus::Running | EngineStatus::Starting
    )
}
