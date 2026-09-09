#!/usr/bin/env node
/* ---------------- Bentomux agent hook CLI ----------------
   Runs inside the agent's terminal (env BENTOMUX_PANE_ID / BENTOMUX_BRIDGE are
   injected by Bentomux's pty spawner). Reads one hook payload JSON from
   stdin, forwards it to the Bentomux bridge as a single JSON line, and —
   for blocking events like PermissionRequest — echoes the bridge's one
   decision line back to stdout.
   When the bridge is unreachable (Bentomux closed) a PermissionRequest
   still fires a plain OS toast as a nudge; the agent itself falls back
   to its native prompt, which stays answerable in the terminal.
   Every failure path exits 0 with no stdout so the agent never breaks. */

'use strict';

const net = require('net');
const { spawn } = require('child_process');

const ADDR_ENV = 'BENTOMUX_BRIDGE';
const PANE_ENV = 'BENTOMUX_PANE_ID';

const psQuote = s => String(s).replace(/'/g, "''");

/* WinRT toast via PowerShell — no modules, fire-and-forget. Windows only;
   other platforms fail silently (spawn error is swallowed). */
function nativeToast(title, body) {
  const script = [
    "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null",
    "$t=[Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)",
    "$x=$t.GetElementsByTagName('text')",
    "$x.Item(0).AppendChild($t.CreateTextNode('" + psQuote(title) + "'))|Out-Null",
    "$x.Item(1).AppendChild($t.CreateTextNode('" + psQuote(body) + "'))|Out-Null",
    "[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Bentomux').Show([Windows.UI.Notifications.ToastNotification]::new($t))",
  ].join('; ');
  try {
    const child = spawn('powershell.exe',
      ['-NoProfile', '-NonInteractive', '-Command', script],
      { detached: true, stdio: 'ignore', windowsHide: true });
    child.on('error', () => { /* no powershell — stay silent */ });
    child.unref();
  } catch { /* spawning unavailable — stay silent */ }
}

/* short human summary for the toast: bash command raw, else tool name */
function approvalSummary(payload) {
  const input = payload.tool_input;
  const cmd = input && typeof input === 'object' && typeof input.command === 'string'
    ? input.command.trim()
    : '';
  const text = cmd || String(payload.tool_name || 'permission request');
  return text.length > 140 ? text.slice(0, 137) + '…' : text;
}

function nudgeIfBlocked(payload) {
  if (payload.hook_event_name !== 'PermissionRequest') return;
  const tool = payload.tool_name ? String(payload.tool_name) : 'tool';
  nativeToast('Claude Code needs approval', approvalSummary(payload));
}

let raw = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', c => { raw += c; });
process.stdin.on('end', () => {
  const addr = process.env[ADDR_ENV];
  if (!addr) return; /* not launched from Bentomux — never toast */
  let payload;
  try { payload = JSON.parse(raw); } catch { return; }
  if (!payload || typeof payload !== 'object') return;

  const envelope = JSON.stringify({
    v: 1,
    event: typeof payload.hook_event_name === 'string' ? payload.hook_event_name : 'Unknown',
    pane: process.env[PANE_ENV] || null,
    payload,
  }) + '\n';

  const sock = net.connect(addr, () => sock.write(envelope, 'utf8'));
  let buf = '';
  let replied = false;
  sock.on('data', chunk => {
    buf += chunk.toString('utf8');
    const nl = buf.indexOf('\n');
    if (nl === -1) return;
    const line = buf.slice(0, nl);
    if (line) {
      replied = true;
      process.stdout.write(line + '\n');
    }
    sock.destroy();
  });
  /* no decision: Bentomux closed / session gone. The agent falls back to its
     native prompt — surface a toast so the user knows to look. */
  sock.on('error', () => {
    nudgeIfBlocked(payload);
    process.exit(0);
  });
  sock.on('close', () => {
    if (!replied) nudgeIfBlocked(payload);
    process.exit(0);
  });
});
