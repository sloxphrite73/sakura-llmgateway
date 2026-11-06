mod admin;
mod balance;
mod config;
mod free_catalog;
mod logger;
mod protocol;
mod proxy;
mod state;
mod stats;
mod strategy;
mod tokenizer;
mod ui;

use axum::routing::{delete, get, post, put};
use axum::Router;
use state::App;
use std::path::PathBuf;
use std::sync::Arc;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Candidate config paths, in preference order, for the implicit (`--config`
/// absent) case. The gateway writes `gateway.json` next to itself on first run,
/// so a directory that denies that write kills startup — and on locked-down
/// machines the exe's own folder can be exactly such a directory (observed:
/// "os error 5" with no log file, i.e. even the fallback log write was denied).
/// Trying several locations makes startup work regardless of which one is
/// writable, while keeping the established layout first:
///   1. `./gateway.json`          — dev workflow, existing deployments, Android
///   2. `<exe dir>/gateway.json`  — the normal double-click layout
///   3. `%LOCALAPPDATA%/sakura-llmgateway/gateway.json` — per-user persistent
///      fallback (never %TEMP%: that gets cleaned). Only used when the two
///      natural locations are not writable.
fn config_candidates() -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from("gateway.json")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("gateway.json");
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    let data_dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("sakura-llmgateway");
    let _ = std::fs::create_dir_all(&data_dir);
    let p = data_dir.join("gateway.json");
    if !out.contains(&p) {
        out.push(p);
    }
    out
}

/// Load the config, trying each candidate until one can be created/read. An
/// existing file always wins (never silently relocate an established config).
fn load_config_with_fallback() -> Result<(config::Config, PathBuf), String> {
    let candidates = config_candidates();
    // A file that already exists must be used as-is (even if it fails to parse:
    // moving away from it would be worse than reporting the parse error).
    for p in &candidates {
        if p.exists() {
            return config::Config::load(p)
                .map(|c| (c, p.clone()))
                .map_err(|e| format!("failed to load config {}: {e}", p.display()));
        }
    }
    // Nothing exists yet: use the first location we can actually write.
    let mut errors = Vec::new();
    for p in &candidates {
        match config::Config::load(p) {
            Ok(c) => return Ok((c, p.clone())),
            Err(e) => errors.push(format!("{}: {e}", p.display())),
        }
    }
    Err(format!(
        "no writable config location (tried {}), cwd={}: {}",
        candidates.len(),
        std::env::current_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|_| "?".into()),
        errors.join(" | ")
    ))
}

/// Startup failure a user may need to read: log it (exe dir, else %TEMP%) and,
/// when attached to a terminal (double-click), wait for Enter so the window
/// doesn't vanish before the message can be seen. Never waits when stdio is
/// redirected (service/CI/`start.bat` piping).
fn fatal(msg: &str) -> ! {
    eprintln!("[gateway] {msg}");
    let mut written: Option<PathBuf> = None;
    let mut targets: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            targets.push(dir.join("startup-error.log"));
        }
    }
    let data_dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("sakura-llmgateway");
    let _ = std::fs::create_dir_all(&data_dir);
    targets.push(data_dir.join("startup-error.log"));
    for t in targets {
        if std::fs::write(&t, format!("{msg}\n")).is_ok() {
            written = Some(t);
            break;
        }
    }
    match written {
        Some(p) => eprintln!("[gateway] details written to {}", p.display()),
        None => eprintln!("[gateway] could not write a startup log anywhere"),
    }
    eprintln!(
        "[gateway] hint: run from a writable folder (e.g. Desktop/Documents), or pass \
         --config <writable path>\\gateway.json"
    );
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
    let explicit = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    // Print where we are running from: with three candidate config locations the
    // diagnostics must show which one was actually used (and the CWD/exe path
    // make a permission problem obvious at a glance).
    eprintln!(
        "[gateway] cwd:  {}",
        std::env::current_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|_| "?".into())
    );
    if let Ok(exe) = std::env::current_exe() {
        eprintln!("[gateway] exe:  {}", exe.display());
    }

    // Never refuse to start because the config file cannot be written: a local
    // gateway that runs with an in-memory config is far more useful than a
    // window that flashes an "access denied" and quits (seen on machines where
    // the exe's folder denies file creation). Persistence is the only loss, and
    // the warning says so.
    let (cfg, config_path, persist) = match &explicit {
        Some(p) => match config::Config::load(p) {
            Ok(c) => (c, p.clone(), true),
            Err(e) => fatal(&format!("failed to load config {}: {e}", p.display())),
        },
        None => match load_config_with_fallback() {
            Ok((c, p)) => (c, p, true),
            Err(e) => {
                let path = config_candidates()
                    .pop()
                    .unwrap_or_else(|| PathBuf::from("gateway.json"));
                eprintln!("[gateway] WARNING: {e}");
                eprintln!(
                    "[gateway] WARNING: cannot write a config file — running with the built-in \
                     defaults in memory. Providers/keys/settings added now will be LOST on exit."
                );
                eprintln!(
                    "[gateway] WARNING: to persist, put a writable gateway.json next to the exe, \
                     move the exe to a writable folder, or pass --config <path>."
                );
                (config::Config::default(), path, false)
            }
        },
    };

    println!(
        "[gateway] config file: {}{}",
        config_path.display(),
        if persist { "" } else { " (NOT WRITABLE — in-memory only)" }
    );
    // Bound ports are printed after binding (they may differ from config if the
    // configured port was occupied and the fallback walked to a free one).
    if cfg.auth.enabled {
        println!("[gateway] auth:       ENABLED ({} keys)", cfg.auth.keys.len());
    } else {
        println!("[gateway] auth:       disabled");
    }

    // Runtime log: next to the config when writable, else under %LOCALAPPDATA%.
    // Everything the gateway reports (rotation, cooldowns, fallback counting) is
    // appended here so a closed console doesn't lose the evidence.
    let mut log_path = config_path.with_file_name("sakura-gateway.log");
    if std::fs::OpenOptions::new().create(true).append(true).open(&log_path).is_err() {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("sakura-llmgateway");
        let _ = std::fs::create_dir_all(&dir);
        log_path = dir.join("sakura-gateway.log");
    }
    logger::init(log_path);

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
