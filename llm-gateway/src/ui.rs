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

fn mime_of(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("json") => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

async fn serve_file(name: &str) -> Response {
    match Assets::get(name) {
        Some(content) => (
            StatusCode::OK,
            [("content-type", mime_of(name))],
            content.data.to_vec(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

pub async fn asset(
    State(_app): State<std::sync::Arc<crate::state::App>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    serve_file(&name).await
}

// ---------------------------------------------------------------------------
// Themes
//
// A theme is a directory `themes/<id>/` containing a `theme.json` manifest
// ({name, author, version, description, mode: "light"|"dark", css[], js[],
// preview}) plus the CSS/JS/image files it references. Built-in themes ship
// embedded under static/themes/; an external `themes/` directory next to the
// config file is scanned at request time and OVERRIDES a built-in theme with
// the same id (= directory name), so users can copy-and-modify a default.
// ---------------------------------------------------------------------------

const DEFAULT_THEME: &str = "sakura-dark";

/// Theme ids are directory names; keep them URL- and filesystem-safe.
fn valid_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The external themes directory: `themes/` next to the active config file.
fn external_themes_dir(app: &crate::state::App) -> std::path::PathBuf {
    app.config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("themes")
}

/// Parse + normalize a theme.json manifest. `id` comes from the directory
/// name (never from the file, so the two can't disagree). Missing css
/// defaults to ["theme.css"]; js/preview optional.
fn parse_manifest(id: &str, raw: &[u8]) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_slice(raw).ok()?;
    let css = v
        .get("css")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_else(|| vec![serde_json::json!("theme.css")]);
    Some(serde_json::json!({
        "id": id,
        "name": v.get("name").and_then(|x| x.as_str()).unwrap_or(id),
        "author": v.get("author").and_then(|x| x.as_str()).unwrap_or(""),
        "version": v.get("version").and_then(|x| x.as_str()).unwrap_or(""),
        "description": v.get("description").and_then(|x| x.as_str()).unwrap_or(""),
        "mode": v.get("mode").and_then(|x| x.as_str()).unwrap_or("dark"),
        "css": css,
        "js": v.get("js").and_then(|x| x.as_array()).cloned().unwrap_or_default(),
        "preview": v.get("preview").and_then(|x| x.as_str()).unwrap_or(""),
    }))
}

/// One theme by id: external directory wins over the embedded copy.
fn load_manifest(app: &crate::state::App, id: &str) -> Option<serde_json::Value> {
    let ext = external_themes_dir(app).join(id).join("theme.json");
    if let Ok(raw) = std::fs::read(&ext) {
        if let Some(m) = parse_manifest(id, &raw) {
            return Some(m);
        }
    }
    let embedded = Assets::get(&format!("themes/{id}/theme.json"))?;
    parse_manifest(id, &embedded.data)
}

/// GET /api/themes — every available theme's manifest (external overrides
/// built-in), plus the currently selected id (config theme or default).
pub async fn list_themes(State(app): State<std::sync::Arc<crate::state::App>>) -> Response {
    let mut ids: Vec<String> = Vec::new();
    // Embedded built-ins.
    for f in Assets::iter() {
        if let Some(rest) = f.strip_prefix("themes/") {
            if let Some((id, tail)) = rest.split_once('/') {
                if tail == "theme.json" && valid_theme_id(id) && !ids.iter().any(|x| x == id) {
                    ids.push(id.to_string());
                }
            }
        }
    }
    // External themes/ directory.
    let ext_dir = external_themes_dir(&app);
    if let Ok(rd) = std::fs::read_dir(&ext_dir) {
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let Some(id) = e.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            if valid_theme_id(&id)
                && e.path().join("theme.json").exists()
                && !ids.iter().any(|x| x == &id)
            {
                ids.push(id);
            }
        }
    }
    let themes: Vec<serde_json::Value> = ids
        .iter()
        .filter_map(|id| load_manifest(&app, id))
        .collect();
    let current = app
        .read_config()
        .theme
        .unwrap_or_else(|| DEFAULT_THEME.to_string());
    crate::admin::ok(serde_json::json!({ "current": current, "themes": themes }))
}

/// GET /themes/{id}/{*path} — one file out of a theme directory. External dir
/// first, embedded fallback. Path traversal (`..`, absolute) rejected.
pub async fn theme_asset(
    State(app): State<std::sync::Arc<crate::state::App>>,
    axum::extract::Path((id, path)): axum::extract::Path<(String, String)>,
) -> Response {
    if !valid_theme_id(&id)
        || path.is_empty()
        || path.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        return (StatusCode::BAD_REQUEST, "bad theme path").into_response();
    }
    let ext = external_themes_dir(&app).join(&id).join(&path);
    if ext.is_file() {
        if let Ok(bytes) = std::fs::read(&ext) {
            return (StatusCode::OK, [("content-type", mime_of(&path))], bytes).into_response();
        }
    }
    serve_file(&format!("themes/{id}/{path}")).await
}
