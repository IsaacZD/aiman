//! Cross-host aggregation endpoints.

use axum::extract::State;
use axum::Json;
use reqwest::Method;

use crate::state::AppState;
use crate::types::BenchmarkRecord;

/// Aggregate engine lists across all configured hosts.
pub async fn aggregate_engines(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let hosts = state.hosts.read().await.clone();

    let results = futures_util::future::join_all(hosts.into_iter().map(|host| {
        let client = state.proxy_client.clone();
        async move {
            match client
                .request::<serde_json::Value>(
                    Method::GET,
                    &host.base_url,
                    "/v1/engines",
                    host.api_key.as_deref(),
                    None::<&()>,
                )
                .await
            {
                Ok((_, body)) => {
                    let engines = body.get("engines").cloned().unwrap_or(serde_json::json!([]));
                    serde_json::json!({
                        "host": {
                            "id": host.id,
                            "name": host.name,
                            "base_url": host.base_url
                        },
                        "engines": engines
                    })
                }
                Err(e) => {
                    serde_json::json!({
                        "host": {
                            "id": host.id,
                            "name": host.name,
                            "base_url": host.base_url
                        },
                        "error": e.to_string()
                    })
                }
            }
        }
    }))
    .await;

    Json(serde_json::json!({ "results": results }))
}

/// Aggregate benchmark records across all configured hosts.
pub async fn aggregate_benchmarks(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let hosts = state.hosts.read().await.clone();

    // Read local dashboard benchmarks
    let local_records: Vec<BenchmarkRecord> = aiman_shared::storage::read_jsonl(
        &state.benchmarks_path,
        None,
        Some(500),
    )
    .await
    .unwrap_or_default();

    // Fetch from all hosts
    let results = futures_util::future::join_all(hosts.into_iter().map(|host| {
        let client = state.proxy_client.clone();
        async move {
            match client
                .request::<serde_json::Value>(
                    Method::GET,
                    &host.base_url,
                    "/v1/benchmarks",
                    host.api_key.as_deref(),
                    None::<&()>,
                )
                .await
            {
                Ok((_, body)) => {
                    let records = body.get("records").cloned().unwrap_or(serde_json::json!([]));
                    serde_json::json!({
                        "host": {
                            "id": host.id,
                            "name": host.name,
                            "base_url": host.base_url
                        },
                        "records": records
                    })
                }
                Err(e) => {
                    serde_json::json!({
                        "host": {
                            "id": host.id,
                            "name": host.name,
                            "base_url": host.base_url
                        },
                        "error": e.to_string()
                    })
                }
            }
        }
    }))
    .await;

    Json(serde_json::json!({
        "results": results,
        "local": local_records
    }))
}
