mod admin;
mod balance;
mod config;
mod free_catalog;
mod protocol;
mod proxy;
mod state;
mod stats;
mod strategy;
mod ui;

use axum::routing::{delete, get, post, put};
use axum::Router;
use state::App;
use std::path::PathBuf;
use std::sync::Arc;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Default config path when `--config` is absent.
///
/// 1. `./gateway.json` when it exists, or the CWD is writable — this keeps the
///    dev workflow (`cd llm-gateway && cargo run`), the Android shell and every
///    existing deployment byte-identical in behavior.
/// 2. `<exe dir>/gateway.json` when the CWD is NOT writable. That is the
///    double-click case: the shell's CWD can be anywhere (e.g. System32 from the
///    Run dialog), where creating the config fails and the process used to exit
///    before the window could be read ("闪退").
fn default_config_path() -> PathBuf {
    let cwd_cfg = PathBuf::from("gateway.json");
    if cwd_cfg.exists() || dir_is_writable(std::path::Path::new(".")) {
        return cwd_cfg;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join("gateway.json");
        }
    }
    cwd_cfg
}

/// Cheap writability probe: can we create a file in `dir`?
fn dir_is_writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(".sakura-write-probe");
    match std::fs::write(&probe, b"1") {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Startup failure that a user may need to read: log it next to the exe and,
/// when attached to a terminal (double-click), wait for Enter so the window
/// doesn't vanish before the message can be seen. Never waits when stdio is
/// redirected (service/CI/`start.bat` piping).
fn fatal(msg: &str) -> ! {
    eprintln!("[gateway] {msg}");
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let log = dir.join("startup-error.log");
            let _ = std::fs::write(&log, format!("{msg}\n"));
            eprintln!("[gateway] details written to {}", log.display());
        }
    }
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        eprintln!("[gateway] press Enter to close this window…");
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
    }
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);

    let cfg = match config::Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => fatal(&format!(
            "failed to load config {}: {e}",
            config_path.display()
        )),
    };

    println!("[gateway] config file: {}", config_path.display());
    // Bound ports are printed after binding (they may differ from config if the
    // configured port was occupied and the fallback walked to a free one).
    if cfg.auth.enabled {
        println!("[gateway] auth:       ENABLED ({} keys)", cfg.auth.keys.len());
    } else {
        println!("[gateway] auth:       disabled");
    }

    let app = Arc::new(App::new(cfg, config_path));

    // Local OpenAI-compatible API
    let api = Router::new()
        .route("/v1/chat/completions", post(proxy::chat_completions))
        .route("/v1/messages", post(proxy::anthropic_messages))
        .route("/v1/messages/count_tokens", post(proxy::anthropic_count_tokens))
        .route("/v1/models", get(proxy::list_models))
        // axum's `Json` extractor caps bodies at 2 MiB by default — that rejects
        // long-context requests (≈500 K tokens already overflows) and any request
        // carrying a base64 image. Lift it: the upstream enforces its own limits.
        .layer(axum::extract::DefaultBodyLimit::disable())
        .with_state(app.clone());

    // Admin REST API + Web UI
    let admin = Router::new()
        .route("/", get(ui::index))
        .route("/static/{*name}", get(ui::asset))
        .route("/api/themes", get(ui::list_themes))
        .route("/themes/{id}/{*path}", get(ui::theme_asset))
        .route("/api/providers", get(admin::list_providers).post(admin::create_provider))
        .route(
            "/api/providers/{id}",
            put(admin::update_provider).delete(admin::delete_provider),
        )
        .route("/api/providers/{id}/price-table", put(admin::update_price_table))
        .route("/api/providers/{id}/rate-limits", put(admin::set_rate_limits))
        .route("/api/providers/{id}/models", post(admin::add_model))
        .route(
            "/api/providers/{id}/models/{model_id}",
            delete(admin::delete_model).put(admin::toggle_model),
        )
        .route("/api/providers/{id}/models/import", post(admin::import_models))
        .route(
            "/api/providers/{id}/models/{model_id}/context-length",
            put(admin::set_model_context),
        )
        .route(
            "/api/providers/{id}/models/{model_id}/supports-images",
            put(admin::set_model_images),
        )
        .route(
            "/api/providers/{id}/upstream-models",
            get(admin::upstream_models),
        )
        .route("/api/providers/{id}/keys", post(admin::add_key))
        .route("/api/providers/{id}/keys/{key_id}", delete(admin::delete_key).put(admin::update_key_seed))
        .route("/api/keys/{key_id}/cooldown", delete(admin::clear_key_cooldown))
        .route("/api/providers/{id}/aliases", post(admin::set_alias))
        .route(
            "/api/providers/{id}/aliases/{alias}",
            delete(admin::delete_alias),
        )
        .route("/api/settings", put(admin::update_settings))
        .route("/api/strategy", get(admin::get_strategy).put(admin::update_strategy))
        .route("/api/strategy/dry-run", post(admin::strategy_dry_run))
        .route(
            "/api/model-groups",
            get(admin::list_model_groups).put(admin::update_model_groups),
        )
        .route("/api/status", get(admin::status))
        .route("/api/stats", get(admin::get_stats))
        .route("/api/config/export", get(admin::export_config))
        .route("/api/config/import", post(admin::import_config))
        .route("/api/config/import-legacy", post(admin::import_legacy_config))
        .route("/api/free-catalog", get(free_catalog::get_catalog))
        .route("/api/free-catalog/refresh", post(free_catalog::refresh_catalog))
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

    // Background bill-balance poller (strategy §2.2 `bill_balance`, decisions
    // ③A/④A): every 60s, poll Tier-A providers flagged has_bill_balance_api,
    // normalize to USD, cache via set_bill_balance. Off the request hot path.
    let balance_app = app.clone();
    tokio::spawn(async move {
        loop {
            balance::poll_all(&balance_app).await;
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });

    // Bind with a transient-retry + free-port fallback. A stale twin process
    // (e.g. the Android service racing a restart) may hold the ports for a few
    // seconds — retry the configured port briefly for that. A PERMANENT occupant
    // (another gateway instance, or anything else on the port) used to crash the
    // gateway; now, after the retry window, we walk up (preferred+1, +2, …) and
    // bind the first free port — never crash on AddrInUse. Returns the listener
    // and the port it actually bound.
    async fn bind_with_fallback(preferred: u16) -> std::io::Result<(tokio::net::TcpListener, u16)> {
        // Retry window ~4s total: long enough for a stale twin to release the
        // port (Android service racing a restart), short enough that a genuinely
        // occupied port falls back fast — the old 17.5s budget made a
        // double-click next to a running instance look like a hang (17s × 2
        // ports before the UI port even started binding).
        let mut delay = std::time::Duration::from_millis(300);
        for _ in 0..5 {
            match tokio::net::TcpListener::bind(("127.0.0.1", preferred)).await {
                Ok(l) => return Ok((l, preferred)),
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(std::time::Duration::from_secs(2));
                }
                Err(e) => return Err(e),
            }
        }
        // Configured port still occupied after the retry window → walk up.
        const CAP: u16 = 100;
        for off in 1..=CAP {
            let p = preferred.wrapping_add(off);
            if p == 0 {
                continue;
            }
            match tokio::net::TcpListener::bind(("127.0.0.1", p)).await {
                Ok(l) => {
                    eprintln!("[gateway] port {preferred} occupied; fell back to {p}");
                    return Ok((l, p));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            format!("no free port in {preferred}..={}", preferred.wrapping_add(CAP)),
        ))
    }

    let cfg_api = app.read_config().api_port;
    let cfg_ui = app.read_config().ui_port;
    // Ports exhausted is a user-readable failure too (panic would flash-close).
    let (api_listener, api_port) = match bind_with_fallback(cfg_api).await {
        Ok(v) => v,
        Err(e) => fatal(&format!("failed to bind api port ({cfg_api}): {e}")),
    };
    let (ui_listener, ui_port) = match bind_with_fallback(cfg_ui).await {
        Ok(v) => v,
        Err(e) => fatal(&format!("failed to bind ui port ({cfg_ui}): {e}")),
    };
    app.bound_api_port
        .store(api_port, std::sync::atomic::Ordering::Relaxed);
    app.bound_ui_port
        .store(ui_port, std::sync::atomic::Ordering::Relaxed);

    println!("[gateway] web ui:        http://127.0.0.1:{ui_port}/");
    println!("[gateway] openai api:    http://127.0.0.1:{api_port}/v1");
    println!("[gateway] anthropic api: http://127.0.0.1:{api_port}/v1/messages");
    if api_port != cfg_api || ui_port != cfg_ui {
        eprintln!(
            "[gateway] NOTE: configured ports ({cfg_api}/{cfg_ui}) were occupied; actual ports above."
        );
    }
    println!("[gateway] ready.");
    tokio::try_join!(
        axum::serve(api_listener, api),
        axum::serve(ui_listener, admin)
    )
    .expect("server error");
}
