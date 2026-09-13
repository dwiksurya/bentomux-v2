# Mobile remote control (ZCode-style) via Cloudflare tunnel — HTTPS-only, full control

## Goal
Replace the current dual-path remote (plain-HTTP LAN server + optional cloudflared toggle) with a single HTTPS-only path: when remote access is on, the bundled cloudflared quick tunnel starts automatically and the panel shows one `https://…trycloudflare.com/?t=<token>` QR/URL. The phone gains **full control**: typing into the watched pane, not just watching + approvals. Plain HTTP (`http://192.168.x.x:8765`) becomes unreachable.

## Architecture after the change
```
phone ──wss──> Cloudflare edge ──> cloudflared ──> axum on 127.0.0.1:8765
                                                    (page + /ws, token-gated)
```
The local axum server stays (cloudflared needs a local origin) but binds loopback only. The existing outbound stderr-scraping tunnel code is reused; only its gating and UI placement change.

## 1. Backend — `src-tauri/src/remote.rs`

**Loopback bind**: `TcpListener::bind(("0.0.0.0", port))` → `("127.0.0.1", port)` (remote.rs:379). LAN HTTP goes dead.

**Typing support** — extend the inbound WS protocol in `handle_incoming` (remote.rs:298):
- New message: `{"t":"write","paneId":"…","data":"…"}` → validates the pane is live and calls `PtyManager::write_term(&pane_id, &data)` (pty.rs:232, already input-activity-aware via `note_user_input`).
- `handle_incoming` needs the `AppHandle` — pass `&st.app` through from `client_loop` (WsCtx already carries it).
- Add a `write` validation helper (live-pane check + non-empty data) as a pure function so it's unit-testable.

**Tunnel always-on**: in `set_remote_enabled(on=true)` (remote.rs:653), drop the `tunnel_enabled` pref check — call `start_tunnel` whenever `remote_running()`. Off path already stops both.

**Remove**: `set_tunnel_enabled` (remote.rs:699), `local_urls()` (remote.rs:607), all `tunnel_enabled` reads.

**`pairing_info`** (remote.rs:629): `urls` becomes the single-element tunnel URL list (`https://…trycloudflare.com/?t=<token>`, pending → empty), `qr` its QR. Remove `tunnel_enabled` from `RemotePairing`; keep `tunnel_url`/`tunnel_qr`/`tunnel_error` (frontend uses them for the "Starting secure tunnel…" state).

**Update the module header comment** (remote.rs:1-12): no longer read-only — token now grants typing; document the security implication.

## 2. Prefs & IPC surface
- `src-tauri/src/state.rs:58-72`: remove `tunnel_enabled` from `RemotePrefs`. Old store files with the key still deserialize (serde ignores unknown fields).
- `src-tauri/src/commands.rs:673-678`: remove `remote_set_tunnel_enabled`.
- `src-tauri/src/lib.rs:108`: remove from `invoke_handler`. Exit handler (`stop_tunnel`/`stop_remote`) stays.
- `src/shared/types.ts`: remove `tunnelEnabled` from `RemotePairing` (~line 335) and `remoteSetTunnelEnabled` from the bridge interface (~line 415).
- `src/preload/bentomux.ts:145`: remove the entry.

## 3. Phone page — `resources/remote-page.html`
The page already picks `wss:` on https (line 113-115). Add:
- An input bar pinned below the screen: text field + Send button, plus quick-key buttons for control sequences phones can't type: `Esc`, `Ctrl+C`, `Enter`, `Tab`, ↑/↓ arrows.
- Enter in the field sends the text + `\r` to the watched pane via the new `write` message; quick keys send their escape sequence (`\x1b`, `\x03`, `\r`, `\t`, `\x1b[A`, `\x1b[B`) directly.
- Visual hint that this session has full control.

## 4. Frontend panel — `src/src/views/remote.ts`
- Remove the "Public HTTPS (via Cloudflare)" segmented toggle, `applyTunnelEnabled`, and the duplicate public-URL section (lines 176-197, 51-59).
- Keep `pollTunnel`: after `applyEnabled(true)`, poll `remoteInfo()` every 1s (max ~15 tries) until `tunnelUrl` or `tunnelError` appears — the trycloudflare URL arrives asynchronously from cloudflared's stderr.
- Panel shows: Off/On toggle, port field, the tunnel QR as *the* pairing QR, the https URL + Copy, "Starting secure tunnel…" while pending, `tunnelError` when cloudflared is missing/fails.
- Update hint text: works over the internet, relayed through Cloudflare, token required, **URL grants full control including typing**.

## 5. Tests
- `remote.rs` tests: remove `local_urls_uses_ipv4_non_loopback`; add tests for the new `write`-message validation helper (wrong pane / empty data rejected); keep tunnel URL-parsing, resource-path, token, and ws-auth tests.
- Existing `smoke_socket_conversion` test keeps covering the bind→from_std sequence.

## 6. Verification
- `cargo test` + `cargo build` in `src-tauri`.
- `npm run build` (or `tsc --noEmit`) for the frontend.
- Manual: with remote on — `curl http://127.0.0.1:8765` serves the page; `curl http://<LAN-IP>:8765` from another machine is refused; panel shows the trycloudflare URL; phone scan → watch a pane, type text, see it appear in the terminal.

## Out of scope
- Auto-starting remote on app launch (still user-toggled, unchanged).
- Tab create/close/split from the phone (tab switching is frontend-local state; the phone can already `watch` any live pane).
- Stable tunnel URLs (needs a Cloudflare account + named tunnel — quick tunnel's ephemeral URL is accepted).
- Screen colors/cell-grid (plain-text mirror unchanged).
