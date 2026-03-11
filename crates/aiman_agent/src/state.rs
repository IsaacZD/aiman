use std::sync::Arc;

use tokio::sync::Mutex;

use crate::hardware::HardwareCache;
use crate::supervisor::Supervisor;

#[derive(Clone)]
pub struct AppState {
    pub supervisor: Arc<Supervisor>,
    pub api_key: Option<String>,
    pub hardware_cache: Arc<Mutex<HardwareCache>>,
}
