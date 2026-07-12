//! Release-channel configuration locks (ADR 0014).
//!
//! `tauri.conf.json` is data, so no other test would catch a regressed CSP,
//! a downgraded updater endpoint or version drift between the manifests.
//! These tests read the shipped config verbatim and pin the security-relevant
//! fields; loosening any of them must be a conscious, reviewed change.

use serde_json::Value;

const TAURI_CONF: &str = include_str!("../tauri.conf.json");
const FRONTEND_PACKAGE_JSON: &str = include_str!("../../frontend/package.json");

fn conf() -> Value {
    serde_json::from_str(TAURI_CONF).expect("tauri.conf.json must be valid JSON")
}

#[test]
fn csp_is_present_and_self_scoped() {
    let conf = conf();
    let csp = conf["app"]["security"]["csp"]
        .as_str()
        .expect("csp must be a string, not null (2026-07-04 audit item)");
    assert!(
        csp.starts_with("default-src 'self'"),
        "csp must anchor on default-src 'self', got: {csp}"
    );
    assert!(
        !csp.contains("unsafe-eval"),
        "csp must not allow unsafe-eval"
    );
}

#[test]
fn updater_feed_is_https_and_has_a_pubkey() {
    let conf = conf();
    let updater = &conf["plugins"]["updater"];
    let pubkey = updater["pubkey"].as_str().expect("updater pubkey missing");
    assert!(
        !pubkey.trim().is_empty(),
        "updater pubkey must not be empty"
    );
    let endpoints = updater["endpoints"]
        .as_array()
        .expect("updater endpoints missing");
    assert!(!endpoints.is_empty(), "at least one updater endpoint");
    for ep in endpoints {
        let url = ep.as_str().expect("endpoint must be a string");
        assert!(
            url.starts_with("https://"),
            "updater endpoint must be https, got: {url}"
        );
    }
}

#[test]
fn bundle_matches_the_alpha_release_channel() {
    let conf = conf();
    let targets = conf["bundle"]["targets"]
        .as_array()
        .expect("bundle targets missing");
    let targets: Vec<&str> = targets.iter().filter_map(Value::as_str).collect();
    // NSIS-only on the alpha channel (ADR 0014: WiX rejects prerelease semver).
    assert_eq!(targets, ["nsis"]);
    assert_eq!(
        conf["bundle"]["createUpdaterArtifacts"], true,
        "updater artifacts must be produced, or the self-update chain breaks"
    );
}

#[test]
fn splash_is_the_only_eager_window_and_has_a_non_white_background() {
    let conf = conf();
    let windows = conf["app"]["windows"]
        .as_array()
        .expect("app windows must be configured");
    assert_eq!(
        windows.len(),
        1,
        "only the lightweight splash may be created before initialization"
    );
    let splash = windows.first().expect("the splash window must exist");
    assert_eq!(splash["label"], "splash");
    assert_eq!(splash["url"], "splash.html");
    assert_eq!(
        splash["visible"],
        serde_json::json!(false),
        "the splash must be created hidden and revealed only after its first paint; \
         a visible-at-creation WebView2 window shows a white HWND during engine cold start"
    );
    assert_eq!(
        splash["backgroundColor"],
        serde_json::json!([246, 245, 242, 0]),
        "the splash surface must match Calm and the WebView layer must be transparent"
    );
    assert_ne!(
        splash["backgroundColor"],
        serde_json::json!([255, 255, 255, 0]),
        "a white native surface reintroduces the startup flash"
    );
}

#[test]
fn manifest_versions_are_in_sync() {
    let conf = conf();
    let tauri_version = conf["version"].as_str().expect("version missing");
    assert_eq!(
        tauri_version,
        env!("CARGO_PKG_VERSION"),
        "tauri.conf.json and Cargo.toml versions must match"
    );
    let package: Value =
        serde_json::from_str(FRONTEND_PACKAGE_JSON).expect("package.json must be valid JSON");
    let npm_version = package["version"].as_str().expect("npm version missing");
    assert_eq!(
        npm_version,
        env!("CARGO_PKG_VERSION"),
        "frontend/package.json version must match Cargo.toml"
    );
}
