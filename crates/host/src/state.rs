use std::sync::Arc;

use crate::supervisor::Supervisor;

#[derive(Clone)]
pub struct AppState {
    pub supervisor: Arc<Supervisor>,
    pub api_key: Option<String>,
}
