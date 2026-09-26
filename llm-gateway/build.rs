/// Build script: embed the git tag (or short hash) as `GATEWAY_VERSION` so the
/// web console can display the real release version instead of a hardcoded string.
/// Falls back to `CARGO_PKG_VERSION` when git is unavailable (e.g. a tarball build).
fn main() {
    let version = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=GATEWAY_VERSION={}", version);
    // Re-run when the current HEAD changes so the version stays in sync.
    println!("cargo:rerun-if-changed=.git/HEAD");
}
