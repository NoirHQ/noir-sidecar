use axum::{Router, routing::get};

pub fn create_router() -> Router {
    Router::new().route("/health", get(|| async { "OK" }))
}
