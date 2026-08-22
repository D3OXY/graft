use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

pub async fn fallback(Path(path): Path<String>) -> Response {
    let path = path.trim_start_matches('/');
    respond(path)
}

pub async fn index() -> Response {
    respond("index.html")
}

fn respond(path: &str) -> Response {
    let candidate = if path.is_empty() {
        asset("index.html")
    } else {
        asset(path)
    };
    let Some((bytes, mime)) = candidate.or_else(|| asset("index.html")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime)],
        Body::from(bytes),
    )
        .into_response()
}
