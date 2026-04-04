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
    #[error("docker image already exists")]
    ImageExists,
    #[error("docker image in use")]
    ImageInUse,
    #[error("docker image not found")]
    ImageNotFound,
    #[error("docker image invalid: {0}")]
    ImageInvalid(String),
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
    }
}
