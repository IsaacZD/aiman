#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("engine not found")]
    NotFound,
    #[error("engine already running")]
    AlreadyRunning,
    #[error("engine not running")]
    NotRunning,
    #[error("engine config already exists")]
    ConfigExists,
    #[error("engine config in use")]
    ConfigInUse,
    #[error("engine config invalid: {0}")]
    ConfigInvalid(String),
    #[error("container image already exists")]
    ImageExists,
    #[error("container image in use")]
    ImageInUse,
    #[error("container image not found")]
    ImageNotFound,
    #[error("container image invalid: {0}")]
    ImageInvalid(String),
    #[error("container image not ready")]
    ImageNotReady,
    #[error("container image is already being prepared")]
    ImagePreparing,
    #[error("podman error: {0}")]
    ContainerApi(String),
}

pub fn map_supervisor_error(err: SupervisorError) -> axum::http::StatusCode {
    match err {
        SupervisorError::NotFound => axum::http::StatusCode::NOT_FOUND,
        SupervisorError::AlreadyRunning => axum::http::StatusCode::CONFLICT,
        SupervisorError::NotRunning => axum::http::StatusCode::CONFLICT,
        SupervisorError::ConfigExists => axum::http::StatusCode::CONFLICT,
        SupervisorError::ConfigInUse => axum::http::StatusCode::CONFLICT,
        SupervisorError::ConfigInvalid(_) => axum::http::StatusCode::BAD_REQUEST,
        SupervisorError::ImageExists => axum::http::StatusCode::CONFLICT,
        SupervisorError::ImageInUse => axum::http::StatusCode::CONFLICT,
        SupervisorError::ImageNotFound => axum::http::StatusCode::NOT_FOUND,
        SupervisorError::ImageInvalid(_) => axum::http::StatusCode::BAD_REQUEST,
        SupervisorError::ImageNotReady => axum::http::StatusCode::CONFLICT,
        SupervisorError::ImagePreparing => axum::http::StatusCode::CONFLICT,
        SupervisorError::ContainerApi(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}
