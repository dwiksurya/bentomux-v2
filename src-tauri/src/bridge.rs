/* ---------------- agent hook bridge ----------------
   Rust port of src/main/bridge.ts. Agents configured with Bentomux's
   managed hooks (agent_hooks.rs) run resources/bentomux-hook.cjs on
   PermissionRequest. The CLI forwards the payload here over a unix socket,
   one JSON line per connection. PermissionRequest connections stay open
   until the user decides in the renderer; the directive JSON then goes
   back on the same socket so the agent itself executes the decision — no
   keystroke synthesis. Everything fails open: a dead bridge just means
   the agent falls back to its native prompt. */

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::bridge_config::bridge_address;
use crate::pty::PtyManager;

/* the renderer-facing approval request (shared/types AgentApprovalRequest) */
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentApprovalRequest {
    pub request_id: String,
    pub pane_id: Option<String>,
    pub agent: String,
    pub tool_name: String,
    pub summary: String,
    pub cwd: Option<String>,
    pub session_id: Option<String>,
}

/* agent boundary events surface to the renderer (shared/types
   AgentEventNotice — jump only today) */
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventNotice {
    pub kind: String,
    pub pane_id: Option<String>,
    pub agent: String,
    pub message: String,
    pub cwd: Option<String>,
    pub session_id: Option<String>,
}

struct Pending {
    req: AgentApprovalRequest,
    conn: std::os::unix::net::UnixStream,
}

struct HookReg {
    created: Vec<std::sync::Arc<dyn Fn(&AgentApprovalRequest) + Send + Sync>>,
    closed: Vec<std::sync::Arc<dyn Fn(&str) + Send + Sync>>,
}

struct BridgeState {
    app: Option<tauri::AppHandle>,
    pending: HashMap<String, Pending>,
    hooks: HookReg,
    active_tab_anchor: Option<String>,
}

fn bridge_state() -> &'static Mutex<BridgeState> {
    static STATE: OnceLock<Mutex<BridgeState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(BridgeState {
            app: None,
            pending: HashMap::new(),
            hooks: HookReg { created: Vec::new(), closed: Vec::new() },
            active_tab_anchor: None,
        })
    })
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/* absolute path of the hook CLI agents execute — unpacked next to the
   binary as a Tauri resource (packaged equivalent of process.resourcesPath) */
pub fn hook_script_path(app: &tauri::AppHandle) -> String {
    /* packaged: the resource bundle (electron's process.resourcesPath) */
    if let Ok(dir) = app.path().resource_dir() {
        let bundled = dir.join("bentomux-hook.cjs");
        if bundled.is_file() {
            return bundled.to_string_lossy().into_owned();
        }
    }
    /* dev: resource_dir may not contain the bundled resources yet, so fall
       back to the project's resources/ folder (electron's
       app.getAppPath()/resources in dev) */
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let dev = std::path::Path::new(&manifest).join("../resources/bentomux-hook.cjs");
        if dev.is_file() {
            return dev.to_string_lossy().into_owned();
        }
    }
    "bentomux-hook.cjs".to_string()
}

/* ---------- subscribers (remote monitor) ---------- */

pub fn on_approval_created(cb: impl Fn(&AgentApprovalRequest) + Send + Sync + 'static) {
    bridge_state().lock().unwrap().hooks.created.push(Arc::new(cb));
}

pub fn on_approval_closed(cb: impl Fn(&str) + Send + Sync + 'static) {
    bridge_state().lock().unwrap().hooks.closed.push(Arc::new(cb));
}

/* currently-blocked requests, so a client that connects late (phone
   opened after the agent got stuck) still sees the decision it must make */
pub fn pending_approvals() -> Vec<AgentApprovalRequest> {
    bridge_state().lock().unwrap().pending.values().map(|p| p.req.clone()).collect()
}

/* ---------- helpers ---------- */

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).filter(|s| !s.is_empty()).map(str::to_string)
}

/* one-line summary of what is being approved: bash commands raw, other
   tools as compact JSON */
fn summarize(input: &serde_json::Value) -> String {
    if let Some(c) = input.get("command").and_then(|v| v.as_str()) {
        if !c.trim().is_empty() {
            return c.trim().to_string();
        }
    }
    serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string())
}

/* PermissionRequest decision, per Claude Code hookSpecificOutput schema */
fn directive(decision: bool) -> String {
    let body = if decision {
        serde_json::json!({ "behavior": "allow" })
    } else {
        serde_json::json!({ "behavior": "deny", "message": "Denied from Bentomux", "interrupt": false })
    };
    serde_json::to_string(&serde_json::json!({
        "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": body }
    }))
    .unwrap_or_default()
}

fn emit(event: &str, payload: &impl Serialize) {
    let Some(app) = &bridge_state().lock().unwrap().app else { return };
    let _ = app.emit(event, payload);
}

/* surface an agent boundary event to the renderer (used by the jump
   command) — kept public for commands.rs */
pub fn emit_agent_event(notice: &AgentEventNotice) {
    emit("agent:event", notice);
}

/* ---------- pending lifecycle ---------- */

pub fn close_pending(request_id: &str) {
    let mut st = bridge_state().lock().unwrap();
    if st.pending.remove(request_id).is_none() {
        return;
    }
    let hooks = st.hooks.closed.clone();
    drop(st);
    emit("agent:approvalClosed", &request_id);
    for cb in &hooks {
        cb(request_id);
    }
}

pub fn drop_pane(pane_id: &str) {
    let ids: Vec<String> = bridge_state()
        .lock()
        .unwrap()
        .pending
        .values()
        .filter(|p| p.req.pane_id.as_deref() == Some(pane_id))
        .map(|p| p.req.request_id.clone())
        .collect();
    for id in ids {
        close_pending(&id);
    }
}

pub fn resolve_approval(request_id: &str, decision: bool) -> bool {
    let mut st = bridge_state().lock().unwrap();
    let Some(mut p) = st.pending.remove(request_id) else { return false };
    let hooks = st.hooks.closed.clone();
    drop(st);
    let _ = p.conn.write_all((directive(decision) + "\n").as_bytes());
    let _ = p.conn.shutdown(std::net::Shutdown::Write); /* EOF closes the hook */
    emit("agent:approvalClosed", &request_id);
    for cb in &hooks {
        cb(request_id);
    }
    true
}

/* anchor pane of the tab the renderer currently shows; reported by the
   renderer on every activation so the overlay can stay hidden while the
   user is already looking at the requesting pane */
pub fn set_active_tab_anchor(tab_id: Option<String>) {
    bridge_state().lock().unwrap().active_tab_anchor = tab_id;
}

fn tree_has_leaf_nodes(pane_id: &str) -> bool {
    let st = bridge_state().lock().unwrap();
    let anchor = match &st.active_tab_anchor {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return false,
    };
    let state = match &st.app {
        Some(app) => app.state::<crate::state::AppStateManager>().get_state(),
        _ => return false,
    };
    let tree_of = |rec: &crate::state::TabRec| -> crate::split_tree::PaneNode {
        rec.split_tree.clone().unwrap_or_else(|| crate::split_tree::leaf_node(&rec.id))
    };
    /* find the tab that owns the anchor, then check the pane lands in it */
    let rec = state.open_tabs.iter().find(|r| crate::split_tree::tree_has_leaf(&tree_of(r), &anchor));
    match rec {
        Some(rec) => crate::split_tree::tree_has_leaf(&tree_of(rec), pane_id),
        None => false,
    }
}

/* approval notifications live ONLY in the floating overlay now. When the
   main window is focused AND the requesting pane's tab is on screen the
   user is already looking at it — show nothing at all. */
fn desktop_notify(req: &AgentApprovalRequest) {
    let st = bridge_state().lock().unwrap();
    let Some(app) = st.app.clone() else { return };
    let notify_enabled = {
        let s = app.state::<crate::state::AppStateManager>();
        s.get_state().prefs.notif_enabled.unwrap_or(true)
    };
    if !notify_enabled {
        return;
    }
    let focused = app
        .get_webview_window("main")
        .map(|w| w.is_focused().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
        .unwrap_or(false);
    drop(st);
    if focused && req.pane_id.as_deref().map(tree_has_leaf_nodes).unwrap_or(false) {
        return;
    }
    /* the floating approval overlay is the user-facing notify surface
       (port of Electron's src/main/overlay.ts showApprovalOverlay). Show
       it; the overlay page subscribes to the `agent:approval` event that
       the bridge already emitted for this request. */
    crate::overlay::show_approval_overlay(&app);
}

/* ---------- connection handling ---------- */

fn request_id() -> String {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("ar-{}-{}", to_base36(n), to_base36(now_ms()))
}

fn to_base36(mut n: u64) -> String {
    const CH: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(CH[(n % 36) as usize]);
        n /= 36;
    }
    out.iter().rev().map(|&b| b as char).collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_envelope(raw: &str) -> Option<(String, Option<String>, serde_json::Value)> {
    let obj: serde_json::Value = serde_json::from_str(raw).ok()?;
    let o = obj.as_object()?;
    if o.get("v").and_then(serde_json::Value::as_u64) != Some(1) {
        return None;
    }
    let event = o.get("event").and_then(serde_json::Value::as_str)?.to_string();
    let pane = o.get("pane").and_then(serde_json::Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let payload = o.get("payload")?.clone();
    if !payload.is_object() {
        return None;
    }
    Some((event, pane, payload))
}

fn dispatch(conn: std::os::unix::net::UnixStream, line: &str) {
    let Some((event, pane, payload)) = parse_envelope(line) else {
        let _ = conn.shutdown(std::net::Shutdown::Both);
        return;
    };
    if event != "PermissionRequest" {
        let _ = conn.shutdown(std::net::Shutdown::Both); /* non-blocking events answer nothing */
        return;
    }
    let Some(tool_name) = str_field(&payload, "tool_name") else {
        let _ = conn.shutdown(std::net::Shutdown::Both);
        return;
    };
    let input = payload.get("tool_input").filter(|v| v.is_object()).cloned().unwrap_or(serde_json::json!({}));
    let req = AgentApprovalRequest {
        request_id: request_id(),
        pane_id: pane,
        agent: "claude".to_string(),
        tool_name,
        summary: summarize(&input),
        cwd: str_field(&payload, "cwd"),
        session_id: str_field(&payload, "session_id"),
    };
    let rid = req.request_id.clone();
    {
        let mut st = bridge_state().lock().unwrap();
        st.pending.insert(rid.clone(), Pending { req: req.clone(), conn });
        let created = st.hooks.created.clone();
        drop(st);
        emit("agent:approval", &req);
        for cb in &created {
            cb(&req);
        }
    }
    desktop_notify(&req);
    /* socket stays open — resolve writes the directive, or the process dies */
}

/* the hook sends a single newline-terminated envelope per connection.
   Read it off a clone of the fd so `stream` can be moved into `pending`
   (PermissionRequest) without losing the read half; non-blocking events
   are answered with an immediate close. */
fn handle_connection(stream: std::os::unix::net::UnixStream) {
    let Ok(read) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return; /* EOF before any payload */
    }
    dispatch(stream, line.trim());
}

/* ---------- listener lifecycle ---------- */

fn live_socket(addr: &str) -> bool {
    std::os::unix::net::UnixStream::connect(addr).is_ok()
}

#[cfg(unix)]
pub fn start_bridge(app: tauri::AppHandle, pty: &PtyManager) {
    bridge_state().lock().unwrap().app = Some(app.clone());

    let addr = bridge_address();
    {
        use std::path::Path;
        if Path::new(&addr).exists() {
            if live_socket(&addr) {
                eprintln!("[bentomux] bridge address busy: {addr}");
                return;
            }
            let _ = std::fs::remove_file(&addr); /* stale socket already gone */
        }
    }

    /* drop pending approvals when the requesting terminal pane exits */
    let mut exit_rx = pty.on_term_exit();
    std::thread::spawn(move || loop {
        use tokio::sync::broadcast::error::TryRecvError;
        match exit_rx.try_recv() {
            Ok((pane_id, _)) => drop_pane(&pane_id),
            Err(TryRecvError::Empty) | Err(TryRecvError::Lagged(_)) => std::thread::sleep(std::time::Duration::from_millis(25)),
            Err(TryRecvError::Closed) => break,
        }
    });

    let listener = match std::os::unix::net::UnixListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[bentomux] bridge bind failed on {addr}: {e}");
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    std::thread::spawn(|| handle_connection(s));
                }
                Err(e) => eprintln!("[bentomux] bridge accept error: {e}"),
            }
        }
    });
}

#[cfg(not(unix))]
pub fn start_bridge(_app: tauri::AppHandle, _pty: &PtyManager) {
    /* named-pipe listener is a Windows-only follow-up (bridge-config addresses
       \\.\pipe\bentomux-bridge); not built on non-unix today */
}

pub fn stop_bridge() {
    let mut st = bridge_state().lock().unwrap();
    for (_, p) in st.pending.drain() {
        let _ = p.conn.shutdown(std::net::Shutdown::Both);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base36_roundtrip() {
        assert_eq!(to_base36(0), "0");
        assert_eq!(to_base36(35), "z");
        assert_eq!(to_base36(36), "10");
        assert_eq!(to_base36(46655), "zzz");
    }

    #[test]
    fn summarize_prefers_command_then_json() {
        let cmd = serde_json::json!({ "command": "  ls -la  " });
        assert_eq!(summarize(&cmd), "ls -la");
        let no_cmd = serde_json::json!({ "foo": "bar", "n": 1 });
        assert_eq!(summarize(&no_cmd), r#"{"foo":"bar","n":1}"#);
    }

    #[test]
    fn directive_matches_claude_schema() {
        let allow: serde_json::Value = serde_json::from_str(&directive(true)).unwrap();
        let d = &allow["hookSpecificOutput"]["decision"];
        assert_eq!(d["behavior"], "allow");
        let deny: serde_json::Value = serde_json::from_str(&directive(false)).unwrap();
        let d = &deny["hookSpecificOutput"]["decision"];
        assert_eq!(d["behavior"], "deny");
        assert_eq!(d["interrupt"], false);
    }

    #[test]
    fn parse_envelope_requires_v1_object_payload() {
        assert!(parse_envelope(r#"{"v":1,"event":"PermissionRequest","pane":"t1","payload":{}}"#).is_some());
        assert!(parse_envelope(r#"{"v":2,"event":"PermissionRequest","payload":{}}"#).is_none());
        assert!(parse_envelope(r#"not json"#).is_none());
        assert!(parse_envelope(r#"{"v":1,"event":"PermissionRequest","pane":null,"payload":"str"}"#).is_none());
    }

    #[test]
    fn request_id_has_ar_prefix() {
        let a = request_id();
        let b = request_id();
        assert!(a.starts_with("ar-"));
        assert_ne!(a, b);
    }

    #[test]
    fn str_field_filters_empty() {
        assert_eq!(str_field(&serde_json::json!({"a":"x"}), "a").as_deref(), Some("x"));
        assert_eq!(str_field(&serde_json::json!({"a":""}), "a"), None);
        assert_eq!(str_field(&serde_json::json!({"a":5}), "a"), None);
    }
}