use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

pub async fn index() -> Response {
    serve_file("index.html").await
}

async fn serve_file(name: &str) -> Response {
    match Assets::get(name) {
        Some(content) => {
            let mime = match name.rsplit('.').next() {
                Some("html") => "text/html; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("js") => "application/javascript; charset=utf-8",
                Some("svg") => "image/svg+xml",
                Some("png") => "image/png",
                _ => "application/octet-stream",
            };
            (
                StatusCode::OK,
                [("content-type", mime)],
                content.data.to_vec(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

pub async fn asset(
    State(_app): State<std::sync::Arc<crate::state::App>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    serve_file(&name).await
}
