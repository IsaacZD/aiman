use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    response::Response,
};

use crate::state::AppState;

// Simple bearer-token auth; disabled when AIMAN_API_KEY is unset.
pub async fn auth_middleware(
    State(state): State<AppState>,
    request: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<Response, StatusCode> {
    let Some(expected) = state.api_key else {
        return Ok(next.run(request).await);
    };

    let provided = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    if provided == Some(expected.as_str()) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
