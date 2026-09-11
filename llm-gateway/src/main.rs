mod admin;
mod config;
mod proxy;
mod state;
mod stats;
mod ui;

use axum::routing::{delete, get, post, put};
use axum::Router;
use state::App;
use std::path::PathBuf;
use std::sync::Arc;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("gateway.json"));

    let cfg = match config::Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[gateway] failed to load config: {e}");
            std::process::exit(1);
        }
    };

    println!("[gateway] config file: {}", config_path.display());
    println!("[gateway] web ui:     http://127.0.0.1:{}/", cfg.ui_port);
    println!("[gateway] openai api: http://127.0.0.1:{}/v1", cfg.api_port);
    if cfg.auth.enabled {
        println!("[gateway] auth:       ENABLED ({} keys)", cfg.auth.keys.len());
    } else {
        println!("[gateway] auth:       disabled");
    }

    let app = Arc::new(App::new(cfg, config_path));

    // Local OpenAI-compatible API
    let api = Router::new()
        .route("/v1/chat/completions", post(proxy::chat_completions))
        .route("/v1/models", get(proxy::list_models))
        .with_state(app.clone());

    // Admin REST API + Web UI
    let admin = Router::new()
        .route("/", get(ui::index))
        .route("/static/{*name}", get(ui::asset))
        .route("/api/providers", get(admin::list_providers).post(admin::create_provider))
        .route(
            "/api/providers/{id}",
            put(admin::update_provider).delete(admin::delete_provider),
        )
        .route("/api/providers/{id}/models", post(admin::add_model))
        .route(
            "/api/providers/{id}/models/{model_id}",
            delete(admin::delete_model).put(admin::toggle_model),
        )
        .route("/api/providers/{id}/models/import", post(admin::import_models))
        .route(
            "/api/providers/{id}/upstream-models",
            get(admin::upstream_models),
        )
        .route("/api/providers/{id}/keys", post(admin::add_key))
        .route("/api/providers/{id}/keys/{key_id}", delete(admin::delete_key))
        .route("/api/keys/{key_id}/cooldown", delete(admin::clear_key_cooldown))
        .route("/api/providers/{id}/aliases", post(admin::set_alias))
        .route(
            "/api/providers/{id}/aliases/{alias}",
            delete(admin::delete_alias),
        )
        .route("/api/settings", put(admin::update_settings))
        .route("/api/status", get(admin::status))
        .route("/api/stats", get(admin::get_stats))
        .route("/api/config/export", get(admin::export_config))
        .route("/api/config/import", post(admin::import_config))
        .with_state(app.clone());

    // Background flushers: stats.json every 5s; gateway.json debounced 10s after the
    // last learned-cooldown change (so 429 storms don't churn the config file).
    let stats_app = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            stats_app.flush_stats();
        }
    });
    let config_app = app.clone();
    tokio::spawn(async move {
        let mut last_change: Option<std::time::Instant> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            config_app.flush_config_if_due(&mut last_change);
        }
    });
    // Flush stats on shutdown is best-effort: the 5s loop bounds the loss.

    let api_listener = tokio::net::TcpListener::bind(("127.0.0.1", app.read_config().api_port))
        .await
        .expect("failed to bind api port");
    let ui_listener = tokio::net::TcpListener::bind(("127.0.0.1", app.read_config().ui_port))
        .await
        .expect("failed to bind ui port");

    println!("[gateway] ready.");
    tokio::try_join!(
        axum::serve(api_listener, api),
        axum::serve(ui_listener, admin)
    )
    .expect("server error");
}
