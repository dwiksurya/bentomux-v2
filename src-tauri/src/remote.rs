/* ---------------- remote monitor (phone browser) ----------------
   Rust port of src/main/remote.ts. Optional HTTP + WebSocket server that
   mirrors pane screens to a phone browser and relays agent approvals.
   Serves the pairing page (resources/remote-page.html); every route (page
   and WS) requires the pairing token from prefs.remote.token, embedded in
   the QR URL shown in Settings. Read-only except approve/deny — no
   terminal input, so a leaked token cannot type into your shells. Screen
   text comes from the headless render in detect/screen.rs (plain text,
   escape sequences consumed); watched panes are re-serialized on a fixed
   tick while their output is moving. Remote decisions go through the same
   resolve_approval() the local overlay uses, so every surface closes via
   agent:approvalClosed. */

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State as AxState;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use base64::Engine;
use futures_util::stream::SplitSink;
use futures_util::SinkExt;
use serde::Serialize;
use serde_json::json;
use tauri::Manager;

use crate::state::{AppStateManager, RemotePrefs};

pub const DEFAULT_REMOTE_PORT: u16 = 8765;

/* watched panes re-serialize at most this often, and only while output
   is actually moving */
const WATCH_TICK_MS: u64 = 250;

/* ---------------- renderer-facing types (shared/types) ---------------- */

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemotePaneInfo {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub state: Option<crate::runtime::AgentRunState>,
    pub runtime: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairing {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub token: String,
    pub urls: Vec<String>,
    pub qr: Option<String>,
    pub error: Option<String>,
}

/* ---------------- outbound message to phone clients ---------------- */

#[derive(Clone)]
enum RemoteMsg {
    /* already-serialized JSON: hello / panes / status / approval / approvalClosed */
    Json(String),
    View { pane_id: String, text: String },
    Gone { pane_id: String },
}

fn js(v: &impl Serialize) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string())
}

/* live server handle; None while stopped. LAST_ERROR remembers the most
   recent bind failure so the Settings surface can show it. */
struct ServerState {
    kill_tx: tokio::sync::watch::Sender<bool>,
    running: Arc<AtomicBool>,
}

static SERVER: std::sync::OnceLock<Mutex<Option<ServerState>>> = std::sync::OnceLock::new();
fn server() -> &'static Mutex<Option<ServerState>> {
    SERVER.get_or_init(|| Mutex::new(None))
}

static LAST_ERROR: std::sync::OnceLock<Mutex<Option<String>>> = std::sync::OnceLock::new();
fn last_error() -> &'static Mutex<Option<String>> {
    LAST_ERROR.get_or_init(|| Mutex::new(None))
}

/* feed callbacks registered once reuse the CURRENT_OUT channel each start,
   so a restart never double-registers or writes into a dead channel. */
static CURRENT_OUT: std::sync::OnceLock<Mutex<Option<tokio::sync::broadcast::Sender<RemoteMsg>>>> =
    std::sync::OnceLock::new();
fn current_out() -> &'static Mutex<Option<tokio::sync::broadcast::Sender<RemoteMsg>>> {
    CURRENT_OUT.get_or_init(|| Mutex::new(None))
}

static FEEDS_ONCE: std::sync::Once = std::sync::Once::new();

pub fn remote_running() -> bool {
    server().lock().unwrap().is_some()
}

fn remote_port(prefs: &crate::state::Prefs) -> u16 {
    prefs.remote.as_ref().and_then(|r| r.port).unwrap_or(DEFAULT_REMOTE_PORT)
}

fn remote_enabled(prefs: &crate::state::Prefs) -> bool {
    prefs.remote.as_ref().and_then(|r| r.enabled).unwrap_or(false)
}

/* generated on first use so paired devices survive restarts */
fn ensure_token(state: &AppStateManager) -> String {
    let now = state.get_state().prefs;
    if let Some(tok) = now.remote.as_ref().and_then(|r| r.token.clone()) {
        return tok;
    }
    let token = random_token();
    let tok = token.clone();
    state.patch_prefs(|p| {
        let r = p.remote.get_or_insert_with(|| RemotePrefs {
            enabled: None,
            port: None,
            token: None,
        });
        r.token = Some(tok);
    });
    token
}

fn random_token() -> String {
    let bytes: [u8; 24] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/* ---------------- pane + status snapshots ---------------- */

fn pane_list(app: &tauri::AppHandle) -> Vec<RemotePaneInfo> {
    use crate::split_tree::{leaf_node, tree_has_leaf};
    let state = app.state::<AppStateManager>().get_state();
    let statuses = crate::runtime::latest_runtime_statuses();
    let mut out = Vec::new();
    for t in app.state::<crate::pty::PtyManager>().live_terms() {
        let rec = state.open_tabs.iter().find(|r| {
            let tree = r.split_tree.clone().unwrap_or_else(|| leaf_node(&r.id));
            tree_has_leaf(&tree, &t.id)
        });
        let Some(rec) = rec else { continue };
        let ws = state.workspaces.iter().find(|w| w.id == rec.workspace_id);
        let status = statuses.get(&t.id);
        out.push(RemotePaneInfo {
            id: t.id,
            title: rec
                .title
                .clone()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| ws.map(|w| w.name.clone()))
                .unwrap_or_else(|| "Terminal".to_string()),
            workspace: ws.map(|w| w.name.clone()).unwrap_or_default(),
            state: status.and_then(|s| s.state.clone()),
            runtime: status.and_then(|s| s.runtime.clone()),
        });
    }
    out
}

/* ---------------- remote page asset ---------------- */

const PAGE_FALLBACK: &str = "<!doctype html><meta charset=utf-8><title>Bentomux remote</title>\
<h1>Bentomux remote</h1><p>Pairing page not found (remote-page.html resource missing).</p>";

fn remote_page_html(app: &tauri::AppHandle) -> String {
    app.path()
        .resource_dir()
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join("remote-page.html")).ok())
        .unwrap_or_else(|| PAGE_FALLBACK.to_string())
}

/* ---------------- http + ws handlers (axum) ---------------- */

#[derive(Clone)]
struct WsCtx {
    token: Arc<String>,
    out: tokio::sync::broadcast::Sender<RemoteMsg>,
    app: tauri::AppHandle,
}

async fn handle_page(
    AxState(st): AxState<WsCtx>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if params.get("t").map(String::as_str) != Some(st.token.as_str()) {
        return (axum::http::StatusCode::UNAUTHORIZED, "Bentomux remote: open the pairing URL shown in Settings → Remote.".to_string()).into_response();
    }
    (
        axum::http::StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        remote_page_html(&st.app),
    )
        .into_response()
}

async fn handle_ws(ws: WebSocketUpgrade, AxState(st): AxState<WsCtx>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| client_loop(socket, st))
}

async fn send_json(sender: &mut SplitSink<WebSocket, Message>, body: String) -> bool {
    sender.send(Message::Text(body)).await.is_ok()
}

async fn client_loop(ws: WebSocket, st: WsCtx) {
    use futures_util::StreamExt;
    let (mut sender, mut receiver) = ws.split();
    let pane_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mut out_rx = st.out.subscribe();

    /* everything a fresh client needs: identity, current state */
    send_json(&mut sender, js(&json!({"t": "hello"}))).await;
    send_json(&mut sender, js(&json!({"t": "panes", "panes": pane_list(&st.app)}))).await;
    send_json(&mut sender, js(&json!({"t": "status", "statuses": crate::runtime::latest_runtime_statuses()}))).await;
    for req in crate::bridge::pending_approvals() {
        send_json(&mut sender, js(&json!({"t": "approval", "req": req}))).await;
    }

    loop {
        tokio::select! {
            inbound = receiver.next() => {
                let Some(incoming) = inbound else { break };
                let msg = match incoming { Ok(m) => m, Err(_) => break };
                match msg {
                    Message::Text(text) => handle_incoming(text, &pane_id, &mut sender).await,
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            out = out_rx.recv() => {
                match out {
                    Ok(RemoteMsg::View { pane_id: p, text }) => {
                        if pane_id.lock().unwrap().as_deref() == Some(p.as_str()) {
                            if !send_json(&mut sender, js(&json!({"t": "view", "paneId": p, "text": text}))).await { break; }
                        }
                    }
                    Ok(RemoteMsg::Gone { pane_id: p }) => {
                        if pane_id.lock().unwrap().as_deref() == Some(p.as_str()) {
                            *pane_id.lock().unwrap() = None;
                            if !send_json(&mut sender, js(&json!({"t": "gone", "paneId": p}))).await { break; }
                        }
                    }
                    Ok(RemoteMsg::Json(body)) => {
                        if !send_json(&mut sender, body).await { break; }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

/* strict wire messages from the phone; anything malformed is ignored */
async fn handle_incoming(
    text: String,
    pane_id: &Arc<Mutex<Option<String>>>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let parsed: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return,
    };
    let Some(m) = parsed.as_object() else { return };
    match m.get("t").and_then(|v| v.as_str()) {
        Some("watch") => {
            if let Some(pid) = m.get("paneId").and_then(|v| v.as_str()) {
                *pane_id.lock().unwrap() = Some(pid.to_string());
                let text = crate::detect::screen::screen_dump(pid);
                let body = js(&json!({"t": "view", "paneId": pid, "text": text}));
                let _ = sender.send(Message::Text(body)).await;
            }
        }
        Some("unwatch") => {
            *pane_id.lock().unwrap() = None;
        }
        Some("approve") => {
            let rid = m.get("requestId").and_then(|v| v.as_str()).map(String::from);
            let decision = m.get("decision").and_then(|v| v.as_str());
            if let (Some(rid), Some(dec @ ("allow" | "deny"))) = (rid, decision) {
                crate::bridge::resolve_approval(&rid, dec == "allow");
            }
        }
        _ => {}
    }
}

/* ---------------- feeds (pty ticks, approvals, runtime status) ---------------- */

fn broadcast(msg: RemoteMsg) {
    if let Some(tx) = current_out().lock().unwrap().as_ref() {
        let _ = tx.send(msg);
    }
}

fn broadcast_view(pane_id: &str, text: &str) {
    broadcast(RemoteMsg::View { pane_id: pane_id.to_string(), text: text.to_string() });
}

/* serialize each watched, recently-active pane once per tick */
fn push_dirty(dirty: &Arc<Mutex<Vec<String>>>) {
    let pending: Vec<String> = dirty.lock().unwrap().drain(..).collect();
    for pane_id in pending {
        broadcast_view(&pane_id, &crate::detect::screen::screen_dump(&pane_id));
    }
}

/* bridge + runtime deliver via push callbacks; wire them once. Each reads
   CURRENT_OUT so a restart reuses the live channel without re-subscribing. */
fn ensure_feeds() {
    FEEDS_ONCE.call_once(|| {
        crate::bridge::on_approval_created(move |req| {
            broadcast(RemoteMsg::Json(js(&json!({"t": "approval", "req": req}))));
        });
        crate::bridge::on_approval_closed(move |request_id| {
            broadcast(RemoteMsg::Json(js(&json!({"t": "approvalClosed", "requestId": request_id}))));
        });
        crate::runtime::on_runtime_update(move |_| {
            broadcast(RemoteMsg::Json(js(&json!({"t": "status", "statuses": crate::runtime::latest_runtime_statuses()}))));
        });
    });
}

/* ---------------- start / stop ---------------- */

pub fn start_remote(app: &tauri::AppHandle, state: &AppStateManager) {
    if remote_running() {
        return;
    }
    ensure_feeds();
    let token = ensure_token(state);
    let port = remote_port(&state.get_state().prefs);

    /* bind synchronously so EADDRINUSE surfaces immediately (like the TS
       listen callback that records lastError before resolving) */
    let listener = match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            *last_error().lock().unwrap() = Some(format!("Port {port} is already in use"));
            return;
        }
        Err(e) => {
            *last_error().lock().unwrap() = Some(e.to_string());
            return;
        }
    };
    let tokio_listener = match tokio::net::TcpListener::from_std(listener) {
        Ok(l) => l,
        Err(e) => {
            *last_error().lock().unwrap() = Some(e.to_string());
            return;
        }
    };
    *last_error().lock().unwrap() = None;

    let (out_tx, _) = tokio::sync::broadcast::channel::<RemoteMsg>(64);
    let dirty: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));
    let (kill_tx, kill_rx) = tokio::sync::watch::channel(false);
    *current_out().lock().unwrap() = Some(out_tx.clone());

    let ctx = WsCtx {
        token: Arc::new(token),
        out: out_tx.clone(),
        app: app.clone(),
    };
    let router = Router::new().route("/", get(handle_page)).route("/ws", get(handle_ws)).with_state(ctx);

    /* subscribe to pty output/exit now (synchronously), move receivers in */
    let app1 = app.clone();
    let d1 = dirty.clone();
    tauri::async_runtime::spawn(async move {
        let mut rx = app1.state::<crate::pty::PtyManager>().on_term_data();
        loop {
            match rx.recv().await {
                Ok((id, _)) => d1.lock().unwrap().push(id),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            }
        }
    });
    let d2 = dirty.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut rx = app2.state::<crate::pty::PtyManager>().on_term_exit();
        loop {
            match rx.recv().await {
                Ok((id, _)) => {
                    d2.lock().unwrap().retain(|p| p != &id);
                    broadcast(RemoteMsg::Json(js(&json!({"t": "panes", "panes": pane_list(&app2)}))));
                    broadcast(RemoteMsg::Gone { pane_id: id });
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            }
        }
    });

    /* fixed tick while the server runs; exits on stop */
    let d3 = dirty.clone();
    let run3 = running.clone();
    std::thread::spawn(move || {
        while run3.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(WATCH_TICK_MS));
            push_dirty(&d3);
        }
    });

    let run_serve = running.clone();
    tauri::async_runtime::spawn(async move {
        let _ = axum::serve(tokio_listener, router)
            .with_graceful_shutdown(async move {
                let mut rx = kill_rx;
                while rx.changed().await.is_ok() && !*rx.borrow() {}
            })
            .await;
        run_serve.store(false, Ordering::SeqCst);
    });

    *server().lock().unwrap() = Some(ServerState {
        kill_tx,
        running,
    });
}

pub fn stop_remote() {
    let st = server().lock().unwrap().take();
    let Some(st) = st else { return };
    let _ = st.kill_tx.send(true);
    st.running.store(false, Ordering::SeqCst);
    *current_out().lock().unwrap() = None;
}

/* ---------------- settings surface ---------------- */

fn local_urls(port: u16, token: &str) -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(list) = local_ip_address::list_afinet_netifas() {
        for (_name, ip) in list {
            if ip.is_ipv4() && !ip.is_loopback() {
                urls.push(format!("http://{ip}:{port}/?t={token}"));
            }
        }
    }
    urls
}

fn qr_svg(url: &str) -> Option<String> {
    let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(240, 240)
        .build();
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    Some(format!("data:image/svg+xml;base64,{b64}"))
}

pub fn pairing_info(state: &AppStateManager) -> RemotePairing {
    let prefs = state.get_state().prefs;
    let port = remote_port(&prefs);
    let token = prefs.remote.as_ref().and_then(|r| r.token.clone()).unwrap_or_default();
    let urls = local_urls(port, &token);
    let qr = if urls.is_empty() { None } else { qr_svg(&urls[0]) };
    let error = last_error().lock().unwrap().clone();
    RemotePairing {
        enabled: remote_enabled(&prefs),
        running: remote_running(),
        port,
        token,
        urls,
        qr,
        error,
    }
}

pub fn set_remote_enabled(app: &tauri::AppHandle, state: &AppStateManager, on: bool) -> RemotePairing {
    state.patch_prefs(|p| {
        let r = p.remote.get_or_insert_with(|| RemotePrefs {
            enabled: None,
            port: None,
            token: None,
        });
        r.enabled = Some(on);
    });
    if on {
        start_remote(app, state);
    } else {
        stop_remote();
    }
    pairing_info(state)
}

pub fn set_remote_port(app: &tauri::AppHandle, state: &AppStateManager, port: u16) -> RemotePairing {
    state.patch_prefs(|p| {
        let r = p.remote.get_or_insert_with(|| RemotePrefs {
            enabled: None,
            port: None,
            token: None,
        });
        r.port = Some(port);
    });
    if remote_running() {
        stop_remote();
        start_remote(app, state);
    }
    pairing_info(state)
}

/* ---------------- tests ---------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_token_is_unique_and_url_safe() {
        let a = random_token();
        let b = random_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32); // 24 bytes base64url → 32 chars
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn local_urls_uses_ipv4_non_loopback() {
        let urls = local_urls(8765, "tok");
        assert!(!urls.is_empty());
        assert!(urls.iter().all(|u| u.contains(":8765/") && u.ends_with("t=tok") && !u.contains("127.0.0.1")));
    }

    #[test]
    fn remote_pane_info_serializes_camel_case() {
        let p = RemotePaneInfo {
            id: "t-1".into(),
            title: "Main".into(),
            workspace: "proj".into(),
            state: Some(crate::runtime::AgentRunState::Working),
            runtime: Some("claude".into()),
        };
        let s = js(&p);
        assert!(s.contains("\"id\":\"t-1\""));
        assert!(s.contains("\"state\":\"working\""));
        assert!(!s.contains('_'));
    }
}