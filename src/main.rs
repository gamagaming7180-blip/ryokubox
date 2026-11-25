//! Ryoku Box — a fresh, single-binary Lithic companion agent. GPL-3.0-or-later.

mod audio;
mod auth;
mod connections;
mod shortcuts;
mod wallpaper;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{sse, Html, IntoResponse, Response, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use auth::{bearer_ok, ct_eq, is_loopback, random_hex, PairState, PAIR_TTL_SECS};
use shortcuts::Shortcut;
use wallpaper::{Detected, Palette};

const PORT: u16 = 8875;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode { Usb, Auto }

#[derive(Clone, Serialize)]
struct Event {
    version: u64,
    wallpaper_url: String,
    palette: Palette,
}

#[derive(Debug)]
struct WallState {
    detected: Option<Detected>,
    palette: Palette,
    fingerprint: u128,
    version: u64,
}

#[derive(Clone)]
struct AppState {
    token: Arc<RwLock<String>>,
    pair: Arc<Mutex<PairState>>,
    wall: Arc<Mutex<WallState>>,
    shortcuts: Arc<RwLock<Vec<Shortcut>>>,
    pending: Arc<RwLock<Vec<Shortcut>>>,
    tx: broadcast::Sender<Event>,
    lan_ip: String,
    mode: Mode,
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}

fn authorized(headers: &HeaderMap, st: &AppState) -> bool {
    let guard = st.token.read().unwrap();
    bearer_ok(headers, &guard)
}

fn current_event(st: &AppState) -> Event {
    let w = st.wall.lock().unwrap();
    Event {
        version: w.version,
        wallpaper_url: format!("/api/wallpaper?v={}", w.version),
        palette: w.palette.clone(),
    }
}

fn token_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    std::path::PathBuf::from(home).join(".config/ryokubox/token")
}

/// The permanent bearer token survives restarts so a paired phone stays paired.
fn load_or_create_token() -> String {
    let path = token_path();
    if let Ok(raw) = std::fs::read_to_string(&path) {
        let t = raw.trim();
        if t.len() >= 32 { return t.to_string(); }
    }
    let token = random_hex(32);
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let _ = std::fs::write(&path, &token);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    token
}

fn save_token(token: &str) {
    let path = token_path();
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let _ = std::fs::write(&path, token);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
}

/// Resolve the LAN address clients should use in the QR code.
fn lan_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0").ok()
        .and_then(|s| { s.connect("8.8.8.8:80").ok()?; s.local_addr().ok() })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".into())
}

fn pair_code(st: &AppState) -> String {
    let mut p = st.pair.lock().unwrap();
    match p.code.clone() {
        Some(c) if p.expires_at.map(|e| e > std::time::Instant::now()).unwrap_or(false) => c,
        _ => p.issue(),
    }
}

fn pair_url(st: &AppState) -> String {
    format!("http://{}:{}/?pair={}", st.lan_ip, PORT, pair_code(st))
}

/// USB variant: works through `adb reverse tcp:8875 tcp:8875` even when LAN is firewalled.
fn pair_url_usb(st: &AppState) -> String {
    format!("http://127.0.0.1:{}/?pair={}", PORT, pair_code(st))
}

fn qr_svg(url: &str) -> Option<String> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::new(url).ok()?;
    let mut renderer = code.render::<svg::Color>();
    renderer
        .min_dimensions(256, 256)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"));
    Some(renderer.build())
}

fn console_html(st: &AppState) -> String {
    let token = st.token.read().unwrap().clone();
    let url = pair_url(st);
    let mode = if st.mode == Mode::Usb { "USB only (127.0.0.1)" } else { "Wi-Fi / Auto (0.0.0.0)" };
    format!(r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Ryoku Box Console</title>
<style>
:root{{color-scheme:dark;font-family:monospace;background:#000;color:#cdc4ba}}
body{{max-width:760px;margin:34px auto;padding:0 21px;line-height:1.6}}
h1{{font-size:28px;color:#ce5e32}} h2{{font-size:15px;letter-spacing:.1em;color:#b0a9a0;margin-top:34px}}
.box{{border:1px solid #46433f;padding:21px;margin:13px 0}}
code{{color:#ce5e32;word-break:break-all}} button{{background:#ce5e32;color:#000;border:0;padding:8px 13px;cursor:pointer;font:inherit;margin-right:8px}}
img{{background:#fff;padding:8px}} .muted{{color:#958f87;font-size:13px}}
</style></head><body>
<h1>力 Ryoku Box Console</h1>
<p class="muted">Loopback only · {mode} · pairing code expires in {ttl}s</p>
<p><a href="/RyokuBox.apk">Download the Android APK</a> — install it on your phone, then pair with the QR below.</p>
<div class="box"><h2>PAIR BY QR — USB</h2><p class="muted">Run <code>adb reverse tcp:8875 tcp:8875</code> first, then scan.</p><img src="/console/qr-usb.svg" alt="USB pairing QR" width="256" height="256">
<p>Or open <code>{usb_url}</code></p></div>
<div class="box"><h2>PAIR BY QR — WI-FI</h2><img src="/console/qr.svg" alt="Wi-Fi pairing QR" width="256" height="256">
<p>Or open <code>{url}</code></p></div>
<div class="box"><h2>PERMANENT BEARER TOKEN</h2><code id="tok">{token}</code>
<p><button onclick="rotate()">Rotate token</button></p></div>
<div class="box"><h2>PENDING SHORTCUT APPROVALS</h2><div id="pending">Loading…</div>
<p><button onclick="approve()">Approve all</button></p></div>
<script>
async function rotate(){{const r=await fetch('/api/token/rotate',{{method:'POST'}});const j=await r.json();document.getElementById('tok').textContent=j.token;}}
async function load(){{const r=await fetch('/api/shortcuts');const j=await r.json();const d=document.getElementById('pending');d.textContent = j.pending&&j.pending.length ? j.pending.map(s=>s.label+' ('+s.argv.join(' ')+')').join(' · ') : 'None';}}
async function approve(){{await fetch('/console/approve-all',{{method:'POST'}});load();}}
load();
</script></body></html>"#,
        mode = mode, ttl = PAIR_TTL_SECS, url = url,
        usb_url = pair_url_usb(st), token = token)
}

async fn root() -> Html<&'static str> {
    Html(include_str!("../panel/index.html"))
}

/// The signed APK, so a phone can install the deck straight from the browser.
async fn apk_download() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/vnd.android.package-archive"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"RyokuBox.apk\""),
        ],
        include_bytes!("../android/RyokuBox.apk").as_slice(),
    ).into_response()
}

async fn manifest() -> Response {
    ([(header::CONTENT_TYPE, "application/manifest+json")], include_str!("../panel/manifest.json")).into_response()
}

async fn console_handler(
    connect: ConnectInfo<SocketAddr>,
    State(st): State<AppState>,
) -> Response {
    if !is_loopback(connect) { return StatusCode::FORBIDDEN.into_response(); }
    Html(console_html(&st)).into_response()
}

async fn console_qr(
    connect: ConnectInfo<SocketAddr>,
    State(st): State<AppState>,
) -> Response {
    if !is_loopback(connect) { return StatusCode::FORBIDDEN.into_response(); }
    match qr_svg(&pair_url(&st)) {
        Some(svg) => ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "no-store")], svg).into_response(),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn console_qr_usb(
    connect: ConnectInfo<SocketAddr>,
    State(st): State<AppState>,
) -> Response {
    if !is_loopback(connect) { return StatusCode::FORBIDDEN.into_response(); }
    match qr_svg(&pair_url_usb(&st)) {
        Some(svg) => ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "no-store")], svg).into_response(),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Deserialize)]
struct PairReq { code: String }

async fn pair(State(st): State<AppState>, Json(body): Json<PairReq>) -> Response {
    let token = st.token.read().unwrap().clone();
    // A device can present its own permanent token as a self-pairing key, so the
    // app never needs a code typed by hand. Constant-time, same as bearer auth.
    let ok = st.pair.lock().unwrap().redeem(&body.code) || ct_eq(&token, &body.code);
    if !ok { return (StatusCode::UNAUTHORIZED, Json(json!({"error":"invalid or expired code"}))).into_response(); }
    Json(json!({ "token": token })).into_response()
}

async fn rotate_token(
    connect: ConnectInfo<SocketAddr>,
    State(st): State<AppState>,
) -> Response {
    if !is_loopback(connect) { return StatusCode::FORBIDDEN.into_response(); }
    let new = random_hex(32);
    save_token(&new);
    *st.token.write().unwrap() = new.clone();
    Json(json!({ "token": new })).into_response()
}

async fn approve_all(
    connect: ConnectInfo<SocketAddr>,
    State(st): State<AppState>,
) -> Response {
    if !is_loopback(connect) { return StatusCode::FORBIDDEN.into_response(); }
    let pending = { let p = st.pending.read().unwrap().clone(); p };
    if !pending.is_empty() {
        let mut list = st.shortcuts.write().unwrap();
        for sc in pending {
            list.retain(|x| x.id != sc.id);
            list.push(Shortcut { approved: true, ..sc });
        }
        let snapshot = list.clone();
        let _ = shortcuts::save_shortcuts(&shortcuts::configured_path(), &snapshot);
        *st.pending.write().unwrap() = Vec::new();
    }
    Json(json!({ "ok": true })).into_response()
}

async fn wallpaper_endpoint(headers: HeaderMap, State(st): State<AppState>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    let detected = st.wall.lock().unwrap().detected.clone();
    let Some(d) = detected else { return (StatusCode::NOT_FOUND, "no wallpaper").into_response() };
    match wallpaper::serve_image(&d).await {
        Some(img) => ([(header::CONTENT_TYPE, HeaderValue::from_str(&img.mime).unwrap_or_else(|_| HeaderValue::from_static("image/jpeg"))),
                       (header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], img.bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn events(headers: HeaderMap, State(st): State<AppState>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    let rx = st.tx.subscribe();
    let initial = current_event(&st);
    let first = tokio_stream::once(Ok::<_, Infallible>(
        sse::Event::default().data(serde_json::to_string(&initial).unwrap())
    ));
    let rest = BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(ev) => Some(Ok::<_, Infallible>(sse::Event::default().data(serde_json::to_string(&ev).unwrap()))),
            Err(_) => None,
        }
    });
    Sse::new(first.chain(rest)).into_response()
}

async fn connections_get(headers: HeaderMap, State(st): State<AppState>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    Json(connections::state().await).into_response()
}

#[derive(Deserialize)]
struct RadioCmd { enabled: bool }

async fn connections_set(
    headers: HeaderMap,
    State(st): State<AppState>,
    Path(target): Path<String>,
    Json(cmd): Json<RadioCmd>,
) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    let ok = connections::set(&target, cmd.enabled).await;
    if ok { Json(json!({ "ok": true })).into_response() }
    else { (StatusCode::BAD_REQUEST, Json(json!({ "ok": false }))).into_response() }
}

async fn vitals(headers: HeaderMap, State(st): State<AppState>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    Json(audio::vitals().await).into_response()
}

async fn audio_get(headers: HeaderMap, State(st): State<AppState>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    Json(audio::audio_state().await).into_response()
}

#[derive(Deserialize)]
struct AudioCmd {
    volume: Option<u32>,
    mute: Option<bool>,
    toggle: Option<bool>,
}

async fn audio_post(headers: HeaderMap, State(st): State<AppState>, Json(cmd): Json<AudioCmd>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    let (ok, error) = audio::set_audio(cmd.volume, cmd.mute, cmd.toggle.unwrap_or(false)).await;
    Json(json!({ "ok": ok, "error": error })).into_response()
}

async fn media_action(headers: HeaderMap, State(st): State<AppState>, Path(action): Path<String>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    let ok = audio::media(&action).await;
    if ok { Json(json!({ "ok": true })).into_response() }
    else { (StatusCode::BAD_REQUEST, Json(json!({ "ok": false }))).into_response() }
}

#[derive(Deserialize)]
struct ArtQuery { url: String }

async fn art(headers: HeaderMap, State(st): State<AppState>, Query(q): Query<ArtQuery>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    if !audio::art_url_allowed(&q.url) { return StatusCode::FORBIDDEN.into_response(); }
    match audio::fetch_art(&q.url).await {
        Some((bytes, mime)) => ([(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")))], bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn session_action(headers: HeaderMap, State(st): State<AppState>, Path(action): Path<String>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    let ok = audio::session(&action).await;
    if ok { Json(json!({ "ok": true })).into_response() }
    else { (StatusCode::BAD_REQUEST, Json(json!({ "ok": false }))).into_response() }
}

#[derive(Serialize)]
struct ShortcutsResponse {
    shortcuts: Vec<Shortcut>,
    pending: Vec<Shortcut>,
}

async fn shortcuts_get(headers: HeaderMap, State(st): State<AppState>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    Json(ShortcutsResponse {
        shortcuts: st.shortcuts.read().unwrap().clone(),
        pending: st.pending.read().unwrap().clone(),
    }).into_response()
}

async fn shortcuts_put(
    headers: HeaderMap,
    connect: ConnectInfo<SocketAddr>,
    State(st): State<AppState>,
    Json(list): Json<Vec<Shortcut>>,
) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    let from_loopback = is_loopback(connect);
    let next: Vec<Shortcut> = list.into_iter()
        .map(|sc| Shortcut { approved: from_loopback, ..sc })
        .collect();
    if from_loopback {
        *st.shortcuts.write().unwrap() = next.clone();
        *st.pending.write().unwrap() = Vec::new();
        let _ = shortcuts::save_shortcuts(&shortcuts::configured_path(), &next);
    } else {
        // LAN edits are held for approval on the PC console; keep existing approved set.
        *st.pending.write().unwrap() = next.clone();
    }
    Json(ShortcutsResponse {
        shortcuts: st.shortcuts.read().unwrap().clone(),
        pending: next,
    }).into_response()
}

async fn run_shortcut(headers: HeaderMap, State(st): State<AppState>, Path(id): Path<String>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    let found = st.shortcuts.read().unwrap().iter().find(|s| s.id == id).cloned();
    // A LAN edit held for approval is known but deliberately not runnable.
    if found.is_none() && st.pending.read().unwrap().iter().any(|s| s.id == id) {
        return (StatusCode::FORBIDDEN, Json(json!({ "ok": false, "error": "awaiting approval" }))).into_response();
    }
    match found {
        Some(sc) if sc.runnable() => {
            match shortcuts::spawn_shortcut(&sc) {
                Ok(()) => Json(json!({ "ok": true })).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "ok": false, "error": e.to_string() }))).into_response(),
            }
        }
        Some(_) => (StatusCode::FORBIDDEN, Json(json!({ "ok": false, "error": "not approved" }))).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "ok": false, "error": "unknown shortcut" }))).into_response(),
    }
}

async fn ping(headers: HeaderMap, State(st): State<AppState>) -> Response {
    if !authorized(&headers, &st) { return unauthorized(); }
    Json(json!({ "ok": true, "version": current_event(&st).version })).into_response()
}

async fn poll_loop(st: AppState) {
    loop {
        let detected = wallpaper::detect().await;
        let fp = wallpaper::palette_fingerprint();
        let changed = {
            let w = st.wall.lock().unwrap();
            let wall_changed = match (&detected, &w.detected) {
                (Some(a), Some(b)) => a != b,
                (Some(_), None) | (None, Some(_)) => true,
                (None, None) => false,
            };
            wall_changed || w.fingerprint != fp
        };
        if changed {
            let event = {
                let mut w = st.wall.lock().unwrap();
                w.detected = detected;
                w.fingerprint = fp;
                w.version += 1;
                w.palette = wallpaper::read_palette();
                Event {
                    version: w.version,
                    wallpaper_url: format!("/api/wallpaper?v={}", w.version),
                    palette: w.palette.clone(),
                }
            };
            let _ = st.tx.send(event);
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
    }
}

fn build_router(st: AppState) -> Router {
    let panel = concat!(env!("CARGO_MANIFEST_DIR"), "/panel");
    Router::new()
        .route("/", get(root))
        .route("/manifest.json", get(manifest))
        .route("/RyokuBox.apk", get(apk_download))
        .route("/sw.js", get(|| async {
            ([(header::CONTENT_TYPE, "application/javascript")], include_str!("../panel/sw.js")).into_response()
        }))
        .route("/console", get(console_handler))
        .route("/console/qr.svg", get(console_qr))
        .route("/console/qr-usb.svg", get(console_qr_usb))
        .route("/console/approve-all", post(approve_all))
        .route("/api/pair", post(pair))
        .route("/api/token/rotate", post(rotate_token))
        .route("/api/ping", get(ping))
        .route("/api/events", get(events))
        .route("/api/wallpaper", get(wallpaper_endpoint))
        .route("/api/vitals", get(vitals))
        .route("/api/connections", get(connections_get))
        .route("/api/connections/:target", post(connections_set))
        .route("/api/audio", get(audio_get).post(audio_post))
        .route("/api/media/:action", post(media_action))
        .route("/api/art", get(art))
        .route("/api/session/:action", post(session_action))
        .route("/api/shortcuts", get(shortcuts_get).put(shortcuts_put))
        .route("/api/run/:id", post(run_shortcut))
        .nest_service("/css", ServeDir::new(format!("{panel}/css")))
        .nest_service("/js", ServeDir::new(format!("{panel}/js")))
        .nest_service("/fonts", ServeDir::new(format!("{panel}/fonts")))
        .nest_service("/icons", ServeDir::new(format!("{panel}/icons")))
        .layer(CorsLayer::permissive())
        .with_state(st)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mode = if args.iter().any(|a| a == "--usb" || a.contains("usb")) {
        Mode::Usb
    } else {
        Mode::Auto
    };

    let ip = lan_ip();
    let (tx, _) = broadcast::channel::<Event>(32);
    let initial_fingerprint = wallpaper::palette_fingerprint();
    let st = AppState {
        token: Arc::new(RwLock::new(load_or_create_token())),
        pair: Arc::new(Mutex::new(PairState::new())),
        wall: Arc::new(Mutex::new(WallState {
            detected: wallpaper::detect().await,
            palette: wallpaper::read_palette(),
            fingerprint: initial_fingerprint,
            version: 1,
        })),
        shortcuts: Arc::new(RwLock::new(shortcuts::load_shortcuts(&shortcuts::configured_path()))),
        pending: Arc::new(RwLock::new(Vec::new())),
        tx,
        lan_ip: ip.clone(),
        mode,
    };

    tokio::spawn(poll_loop(st.clone()));
    let bind_addr: SocketAddr = match mode {
        Mode::Usb => SocketAddr::from(([127, 0, 0, 1], PORT)),
        Mode::Auto => SocketAddr::from(([0, 0, 0, 0], PORT)),
    };

    println!("Ryoku Box 1.0.0 listening on {bind_addr} (LAN {ip}) mode={mode:?}");
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, build_router(st).into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_serializes_expected_fields() {
        let ev = Event {
            version: 7,
            wallpaper_url: "/api/wallpaper?v=7".into(),
            palette: Palette::signature(),
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["version"], 7);
        assert_eq!(v["wallpaper_url"], "/api/wallpaper?v=7");
        assert_eq!(v["palette"]["source"], "signature");
        assert_eq!(v["palette"]["gold"], "#ce5e32");
    }
}
