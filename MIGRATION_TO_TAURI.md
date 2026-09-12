# Bentomux: Complete Electron → Tauri Migration Specification

**Version:** 1.0  
**Date:** 2026-09-08  
**Target:** Zero feature loss, 95% size reduction (106 MB → 8-10 MB)

---

## Migration Goals

### Primary Objectives
1. **100% feature parity** — every capability in Electron version must work in Tauri
2. **Binary size reduction** — 106 MB → 8-10 MB installer
3. **Performance improvement** — faster startup, lower memory usage
4. **Maintain UX** — zero user-visible regressions

### Non-Goals
- UI redesign (keep existing renderer)
- Feature additions
- Architecture changes beyond Electron → Tauri

---

## Current Architecture Inventory

### Project Structure
```
bentomux/
├── src/
│   ├── main/           # Electron main process → REWRITE IN RUST
│   ├── preload/        # Context bridge → REMOVE (Tauri has direct invoke)
│   ├── renderer/       # UI layer → KEEP UNCHANGED
│   └── shared/         # Types + split-tree logic → PORT TO RUST
├── resources/
│   └── bentomux-hook.cjs  # Agent hook CLI → KEEP, adjust paths
├── scripts/            # Test scripts → PORT TO RUST TESTS
└── package.json        # npm → ADD Cargo.toml
```

### Dependencies to Replace
| Electron/Node.js | Tauri/Rust Equivalent |
|------------------|------------------------|
| `electron` | `tauri` |
| `node-pty` | `portable-pty` |
| `ps-list` | `sysinfo` |
| `@xterm/headless` | `vt100` crate OR Node.js subprocess |
| `ws` (WebSocket) | `tokio-tungstenite` |
| Node.js `http` | `axum` or `warp` |
| Node.js `net` | `tokio::net` |
| Node.js `fs` | `std::fs` + `tokio::fs` |
| Node.js `child_process` | `std::process::Command` |

---

## Feature Checklist (Must Preserve)

### Core Terminal Features
- [ ] PTY spawning (shell auto-detect: PowerShell, cmd, Git Bash, WSL)
- [ ] Multi-workspace support (add/remove project folders)
- [ ] Chrome-style terminal tabs with custom titles
- [ ] Recursive split panes (horizontal/vertical, 50/50)
- [ ] Split-tree persistence + restoration across restarts
- [ ] Copy/paste (Ctrl+C with selection / Ctrl+V)
- [ ] Right-click context menu
- [ ] Clickable URLs
- [ ] Hidden scrollbars
- [ ] Terminal resize (cols/rows sync with xterm.js)
- [ ] Terminal exit handling + cleanup

### Git Integration
- [ ] Per-workspace branch detection (fs.watch)
- [ ] Changes pill (+N −N) in titlebar
- [ ] Git panel: branch status + file list
- [ ] Per-file diff view with syntax highlighting (Shiki)
- [ ] Push with upstream tracking
- [ ] Hunks display in diff view

### Agent Runtime Management
- [ ] Detect 9 agent runtimes:
  - Claude Code
  - pi
  - Qwen CLI
  - OpenAI Codex
  - OpenCode
  - Gemini CLI
  - Cursor
  - Kilo Code
  - QwenPaw
- [ ] Per-agent model settings UI (model, baseUrl, context, apiKey, format)
- [ ] Write to native config files per agent
- [ ] Memory/skills/MCP toggles per agent
- [ ] Live runtime status (idle/working/blocked) per terminal tab
- [ ] Process table polling (2s interval)
- [ ] Screen buffer parsing for agent state detection
- [ ] Agent manifest evaluation (herdr-style rules)

### Agent Approval Bridge
- [ ] Named pipe (Windows) / Unix socket server
- [ ] Hook CLI integration (`resources/bentomux-hook.cjs`)
- [ ] Permission request overlay (always-on-top, floating pill)
- [ ] Approve/Deny/Jump-to-tab actions
- [ ] Connection lifecycle (keep socket open until decision)
- [ ] Fail-open behavior (dead bridge → native prompts)
- [ ] Terminal exit cleanup (close pending approvals)
- [ ] Notification sound (chime on approval request)

### Remote Monitor
- [ ] HTTP server (default port 8765)
- [ ] WebSocket server for real-time pane mirroring
- [ ] QR code pairing (token-based auth)
- [ ] Remote pane list (workspace + title + status)
- [ ] Screen text streaming (headless render)
- [ ] Remote approval approve/deny
- [ ] Token persistence across restarts
- [ ] Client connection tracking

### Settings
- [ ] Theme: light/dark
- [ ] Palettes: default, Catppuccin, Rosé Pine, Gruvbox, Dracula, Nord
- [ ] Terminal font family + size
- [ ] Shell selection for new terminals
- [ ] Keybinding customization (palette, splits)
- [ ] Sidebar width persistence
- [ ] Expanded section state
- [ ] Last tab title per workspace
- [ ] Approval overlay size
- [ ] Notification sound toggle
- [ ] Remote server enable/disable + port config

### UI Features
- [ ] Command palette (Ctrl+K) — jump to nav items, panes, tabs
- [ ] Sidebar: workspace list, terminal pane list, git nav, agents nav
- [ ] Titlebar: tabs, changes pill, window controls
- [ ] Split pane controls (split right/down, close pane)
- [ ] Tab rename (double-click or right-click)
- [ ] Workspace add/remove
- [ ] Active tab tracking
- [ ] State persistence (bentomux.json in userData)

---

## Tauri Project Setup

### Step 1: Initialize Tauri Project

```bash
# Install Tauri CLI
cargo install tauri-cli

# Initialize Tauri in existing project
cd bentomux
npm install --save-dev @tauri-apps/cli
npm install @tauri-apps/api

# Create Tauri project structure
npm run tauri init
```

**Configuration prompts:**
- App name: `Bentomux`
- Window title: `Bentomux`
- Web assets path: `out/renderer`
- Dev server URL: `http://localhost:5173`
- Frontend dev command: `npm run dev` (keep electron-vite for now)
- Frontend build command: `npm run build`

### Step 2: Project Structure After Init

```
bentomux/
├── src-tauri/              # NEW: Rust backend
│   ├── Cargo.toml          # Rust dependencies
│   ├── tauri.conf.json     # Tauri config
│   ├── build.rs            # Build script
│   ├── icons/              # App icons
│   └── src/
│       ├── main.rs         # Entry point
│       ├── commands.rs     # IPC command handlers
│       ├── state.rs        # AppState management
│       ├── pty.rs          # PTY manager
│       ├── git.rs          # Git operations
│       ├── runtime.rs      # Agent detection
│       ├── bridge.rs       # Approval bridge
│       ├── remote.rs       # Remote monitor server
│       ├── agents/         # Agent adapters
│       └── detect/         # Screen parsing
├── src/
│   ├── renderer/           # KEEP: Vanilla TS UI (unchanged)
│   └── shared/             # REMOVE: types moved to Rust
├── resources/
│   └── bentomux-hook.cjs   # KEEP: Agent hook CLI
└── package.json
```

### Step 3: Cargo.toml Dependencies

```toml
[package]
name = "bentomux"
version = "0.1.0"
edition = "2021"

[dependencies]
tauri = { version = "2.1", features = ["shell-open"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
portable-pty = "0.8"
sysinfo = "0.32"
notify = "7.0"
axum = "0.7"
tokio-tungstenite = "0.24"
qrcode = "0.14"
image = "0.25"
base64 = "0.22"
rand = "0.8"
chrono = "0.4"
dirs = "5.0"

# Optional: for screen buffer parsing
vt100 = "0.15"

# YAML/TOML parsing for agent configs
serde_yaml = "0.9"
toml = "0.8"

# Cross-platform process handling
[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = ["Win32_System_Pipes", "Win32_Storage_FileSystem"] }

[target.'cfg(unix)'.dependencies]
# Unix socket support included in tokio
```

### Step 4: tauri.conf.json Configuration

```json
{
  "productName": "Bentomux",
  "version": "0.1.0",
  "identifier": "app.bentomux.desktop",
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devUrl": "http://localhost:5173",
    "frontendDist": "../out/renderer"
  },
  "bundle": {
    "active": true,
    "targets": ["nsis", "msi"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "resources": ["../resources/bentomux-hook.cjs"],
    "externalBin": []
  },
  "app": {
    "windows": [
      {
        "title": "Bentomux",
        "width": 1400,
        "height": 900,
        "resizable": true,
        "fullscreen": false
      }
    ],
    "security": {
      "csp": null
    }
  }
}
```

---

## Component Migration Guide

### 1. State Management (store.ts → state.rs)

**Current (Electron):**
```typescript
// src/main/store.ts
interface AppState {
  version: number;
  workspaces: WorkspaceRec[];
  openTabs: TabRec[];
  activeWorkspaceId: string | null;
  shadow: ShadowStore;
  prefs: Prefs;
  agents: AgentInfo[];
}

let state: AppState;

export function getState(): AppState { return state; }
export function patchState(partial: Partial<AppState>): void { ... }
```

**Target (Tauri):**
```rust
// src-tauri/src/state.rs
use serde::{Serialize, Deserialize};
use std::sync::Mutex;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct WorkspaceRec {
    pub id: String,
    pub path: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TabRec {
    pub id: String,
    pub workspace_id: String,
    pub split_tree: Option<PaneNode>,
    pub title: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Prefs {
    pub theme: Option<String>,
    pub palette: Option<String>,
    pub font: Option<String>,
    pub font_size: Option<u32>,
    pub shell: Option<String>,
    pub shortcuts: Option<std::collections::HashMap<String, String>>,
    pub pane_hidden: Option<bool>,
    pub sidebar_width: Option<u32>,
    pub expanded: Option<std::collections::HashMap<String, bool>>,
    pub tab_titles: Option<std::collections::HashMap<String, String>>,
    pub approval_overlay: Option<OverlaySize>,
    pub notif_enabled: Option<bool>,
    pub notif_sound: Option<bool>,
    pub remote: Option<RemotePrefs>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AppState {
    pub version: u32,
    pub workspaces: Vec<WorkspaceRec>,
    pub open_tabs: Vec<TabRec>,
    pub active_workspace_id: Option<String>,
    pub shadow: serde_json::Value,
    pub prefs: Prefs,
    pub agents: Vec<AgentInfo>,
}

pub struct AppStateManager {
    state: Mutex<AppState>,
    path: PathBuf,
}

impl AppStateManager {
    pub fn new(app_handle: &tauri::AppHandle) -> Self {
        let path = app_handle
            .path()
            .app_data_dir()
            .unwrap()
            .join("bentomux.json");
        
        let state = if path.exists() {
            let content = fs::read_to_string(&path).unwrap();
            serde_json::from_str(&content).unwrap_or_else(|_| Self::default_state())
        } else {
            Self::default_state()
        };

        Self {
            state: Mutex::new(state),
            path,
        }
    }

    fn default_state() -> AppState {
        AppState {
            version: 1,
            workspaces: vec![],
            open_tabs: vec![],
            active_workspace_id: None,
            shadow: serde_json::json!({}),
            prefs: Prefs::default(),
            agents: vec![],
        }
    }

    pub fn get_state(&self) -> AppState {
        self.state.lock().unwrap().clone()
    }

    pub fn update_state<F>(&self, updater: F)
    where
        F: FnOnce(&mut AppState),
    {
        let mut state = self.state.lock().unwrap();
        updater(&mut state);
        self.save(&state);
    }

    fn save(&self, state: &AppState) {
        let json = serde_json::to_string_pretty(state).unwrap();
        fs::write(&self.path, json).ok();
    }
}
```


### 3. IPC Commands (ipc.ts → commands.rs)

**Current (Electron):**
```typescript
// src/main/ipc.ts
import { ipcMain } from 'electron';

export function registerIpc(): void {
  ipcMain.handle('workspace:add', async (_, path: string) => {
    const ws = addWorkspace(path);
    return getState();
  });

  ipcMain.handle('tab:create', async (_, workspaceId: string) => {
    const term = createTerm(workspaceId);
    // ... update state
    return getState();
  });

  ipcMain.handle('pty:write', async (_, id: string, data: string) => {
    writeTerm(id, data);
  });
}
```

**Target (Tauri):**
```rust
// src-tauri/src/commands.rs
use tauri::State;
use crate::state::AppStateManager;
use crate::pty::PtyManager;

#[tauri::command]
pub fn workspace_add(
    path: String,
    state: State<'_, AppStateManager>,
) -> Result<AppState, String> {
    state.update_state(|s| {
        let id = format!("ws-{}", chrono::Utc::now().timestamp_millis());
        let name = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Workspace")
            .to_string();
        
        s.workspaces.push(WorkspaceRec { id, path, name });
    });
    Ok(state.get_state())
}

#[tauri::command]
pub fn workspace_remove(
    id: String,
    state: State<'_, AppStateManager>,
) -> Result<AppState, String> {
    state.update_state(|s| {
        s.workspaces.retain(|w| w.id != id);
        s.open_tabs.retain(|t| t.workspace_id != id);
    });
    Ok(state.get_state())
}

#[tauri::command]
pub fn tab_create(
    workspace_id: String,
    state: State<'_, AppStateManager>,
    pty: State<'_, PtyManager>,
) -> Result<AppState, String> {
    let workspace = state
        .get_state()
        .workspaces
        .iter()
        .find(|w| w.id == workspace_id)
        .ok_or("Workspace not found")?
        .clone();
    
    let term_id = pty.create_term(
        workspace_id.clone(),
        workspace.path.clone(),
        "pwsh".to_string(), // TODO: resolve shell from prefs
    )?;
    
    state.update_state(|s| {
        s.open_tabs.push(TabRec {
            id: term_id.clone(),
            workspace_id,
            split_tree: None,
            title: None,
        });
    });
    
    Ok(state.get_state())
}

#[tauri::command]
pub fn pty_write(
    id: String,
    data: String,
    pty: State<'_, PtyManager>,
) -> Result<(), String> {
    pty.write_term(&id, &data)
}

#[tauri::command]
pub fn pty_resize(
    id: String,
    cols: u16,
    rows: u16,
    pty: State<'_, PtyManager>,
) -> Result<(), String> {
    pty.resize_term(&id, cols, rows)
}

#[tauri::command]
pub fn prefs_update(
    prefs: Prefs,
    state: State<'_, AppStateManager>,
) -> Result<AppState, String> {
    state.update_state(|s| {
        s.prefs = prefs;
    });
    Ok(state.get_state())
}

#[tauri::command]
pub fn get_state(state: State<'_, AppStateManager>) -> AppState {
    state.get_state()
}
```

**Renderer Changes (TypeScript):**
```typescript
// src/renderer/src/api.ts - NEW FILE
import { invoke } from '@tauri-apps/api/core';
import type { AppState, Prefs } from './types';

export async function workspaceAdd(path: string): Promise<AppState> {
  return invoke('workspace_add', { path });
}

export async function workspaceRemove(id: string): Promise<AppState> {
  return invoke('workspace_remove', { id });
}

export async function tabCreate(workspaceId: string): Promise<AppState> {
  return invoke('tab_create', { workspaceId });
}

export async function ptyWrite(id: string, data: string): Promise<void> {
  return invoke('pty_write', { id, data });
}

export async function ptyResize(id: string, cols: number, rows: number): Promise<void> {
  return invoke('pty_resize', { id, cols, rows });
}

export async function prefsUpdate(prefs: Prefs): Promise<AppState> {
  return invoke('prefs_update', { prefs });
}

export async function getState(): Promise<AppState> {
  return invoke('get_state');
}
```

**Event Listening (Tauri):**
```typescript
// src/renderer/src/events.ts - NEW FILE
import { listen } from '@tauri-apps/api/event';

export async function setupEventListeners() {
  // PTY data events
  await listen('pty:data', (event) => {
    const [id, data] = event.payload as [string, string];
    // Forward to xterm.js instance
    terminalManager.write(id, data);
  });

  // PTY exit events
  await listen('pty:exit', (event) => {
    const [id, exitCode] = event.payload as [string, number];
    terminalManager.handleExit(id, exitCode);
  });

  // Git branch changes
  await listen('branch', (event) => {
    const [workspaceId, branch] = event.payload as [string, string];
    gitManager.updateBranch(workspaceId, branch);
  });

  // Runtime status updates
  await listen('rt:status', (event) => {
    const statuses = event.payload;
    runtimeStatusManager.update(statuses);
  });

  // Agent approvals
  await listen('agent:approval', (event) => {
    const approval = event.payload;
    approvalOverlay.show(approval);
  });
}
```

**All IPC Commands to Port:**

| Electron Handler | Tauri Command | Priority |
|------------------|---------------|----------|
| `workspace:add` | `workspace_add` | P0 |
| `workspace:remove` | `workspace_remove` | P0 |
| `workspace:list` | Built into `get_state` | P0 |
| `tab:create` | `tab_create` | P0 |
| `tab:close` | `tab_close` | P0 |
| `tab:rename` | `tab_rename` | P1 |
| `tab:split` | `tab_split` | P1 |
| `pty:write` | `pty_write` | P0 |
| `pty:resize` | `pty_resize` | P0 |
| `git:files` | `git_files` | P1 |
| `git:diff` | `git_diff` | P1 |
| `git:push` | `git_push` | P1 |
| `agents:list` | `agents_list` | P2 |
| `agents:config` | `agents_config` | P2 |
| `agents:update-model` | `agents_update_model` | P2 |
| `agents:toggle-resource` | `agents_toggle_resource` | P2 |
| `prefs:update` | `prefs_update` | P1 |
| `approval:resolve` | `approval_resolve` | P2 |
| `remote:pairing` | `remote_pairing` | P2 |

### 4. Git Operations (git.ts, git-ops.ts → git.rs)

**Current (Electron):**
```typescript
// src/main/git-ops.ts
import { execFile } from 'child_process';

export async function gitStatus(cwd: string): Promise<GitStatus> {
  const { stdout } = await execFileAsync('git', ['status', '--porcelain=v1'], { cwd });
  // ... parse output
}

export async function gitDiff(cwd: string, file: string): Promise<string> {
  const { stdout } = await execFileAsync('git', ['diff', 'HEAD', '--', file], { cwd });
  return stdout;
}
```

**Target (Tauri):**
```rust
// src-tauri/src/git.rs
use std::process::Command;
use std::path::Path;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct GitStatus {
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    pub added: u32,
    pub modified: u32,
    pub deleted: u32,
    pub files: Vec<GitFile>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GitFile {
    pub path: String,
    pub status: String, // "M", "A", "D", etc.
}

pub fn git_status(cwd: &str) -> Result<GitStatus, String> {
    let output = Command::new("git")
        .args(&["status", "--porcelain=v1", "--branch"])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Git command failed: {}", e))?;
    
    if !output.status.success() {
        return Err("Not a git repository".into());
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_status(&stdout)
}

fn parse_status(output: &str) -> Result<GitStatus, String> {
    let mut branch = String::from("main");
    let mut ahead = 0;
    let mut behind = 0;
    let mut added = 0;
    let mut modified = 0;
    let mut deleted = 0;
    let mut files = Vec::new();
    
    for line in output.lines() {
        if line.starts_with("##") {
            // Parse branch info: ## main...origin/main [ahead 2, behind 1]
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 1 {
                branch = parts[1].split("...").next().unwrap_or("main").to_string();
            }
            // Parse ahead/behind
            if let Some(bracket) = line.find('[') {
                let info = &line[bracket..];
                if let Some(a) = info.find("ahead") {
                    ahead = info[a+6..].split(|c: char| !c.is_numeric())
                        .next().unwrap_or("0").parse().unwrap_or(0);
                }
                if let Some(b) = info.find("behind") {
                    behind = info[b+7..].split(|c: char| !c.is_numeric())
                        .next().unwrap_or("0").parse().unwrap_or(0);
                }
            }
        } else if line.len() >= 3 {
            let status = &line[0..2];
            let path = line[3..].trim().to_string();
            
            match status.trim() {
                "A" | "??" => added += 1,
                "M" | " M" | "MM" => modified += 1,
                "D" | " D" => deleted += 1,
                _ => {}
            }
            
            files.push(GitFile {
                path,
                status: status.trim().to_string(),
            });
        }
    }
    
    Ok(GitStatus {
        branch,
        ahead,
        behind,
        added,
        modified,
        deleted,
        files,
    })
}

pub fn git_diff(cwd: &str, file: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(&["diff", "HEAD", "--", file])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Git diff failed: {}", e))?;
    
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn git_push(cwd: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(&["push", "-u", "origin", "HEAD"])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Git push failed: {}", e))?;
    
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

**File Watching:**
```rust
// src-tauri/src/git_watcher.rs
use notify::{Watcher, RecursiveMode, Event, EventKind};
use std::sync::mpsc::channel;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn watch_workspace(workspace_id: String, path: String, app_handle: AppHandle) {
    let git_dir = PathBuf::from(&path).join(".git");
    if !git_dir.exists() {
        return;
    }
    
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(tx).unwrap();
    
    watcher.watch(&git_dir.join("HEAD"), RecursiveMode::NonRecursive).ok();
    watcher.watch(&git_dir.join("refs"), RecursiveMode::Recursive).ok();
    
    tokio::spawn(async move {
        loop {
            match rx.recv() {
                Ok(Ok(event)) => {
                    if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        // Read new branch
                        if let Ok(status) = crate::git::git_status(&path) {
                            app_handle
                                .emit_all("branch", (workspace_id.clone(), status.branch))
                                .ok();
                        }
                    }
                }
                _ => break,
            }
        }
    });
}
```
### 2. PTY Management (pty.ts → pty.rs)

**Current (Electron):**
```typescript
// src/main/pty.ts
import { spawn, type IPty } from 'node-pty';

interface Term {
  id: string;
  workspaceId: string;
  pid: number;
  pty: IPty;
  alive: boolean;
}

const terms = new Map<string, Term>();

export function createTerm(workspace: WorkspaceRec): Term {
  const shell = resolveShell(getState().prefs.shell);
  const id = 't-' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
  const pty = spawn(shell.file, shell.args, {
    name: 'xterm-256color',
    cols: 80,
    rows: 24,
    cwd: workspace.path,
    env: { ...process.env, ...bridgeEnvFor(id) },
  });
  
  pty.onData(chunk => send('pty:data', id, chunk));
  pty.onExit(({ exitCode }) => send('pty:exit', id, exitCode));
  
  return term;
}
```

**Target (Tauri):**
```rust
// src-tauri/src/pty.rs
use portable_pty::{native_pty_system, CommandBuilder, PtySize, PtyPair, Child};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::io::{Read, Write};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

pub struct Term {
    pub id: String,
    pub workspace_id: String,
    pub pid: u32,
    pub alive: bool,
    writer: Box<dyn Write + Send>,
}

pub struct PtyManager {
    terms: Arc<Mutex<HashMap<String, Term>>>,
    app_handle: AppHandle,
}

impl PtyManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            terms: Arc::new(Mutex::new(HashMap::new())),
            app_handle,
        }
    }

    pub fn create_term(
        &self,
        workspace_id: String,
        workspace_path: String,
        shell: String,
    ) -> Result<String, String> {
        let id = format!(
            "t-{}{}",
            chrono::Utc::now().timestamp_millis(),
            rand::random::<u32>()
        );

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("PTY creation failed: {}", e))?;

        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&workspace_path);
        cmd.env("TERM", "xterm-256color");
        
        // Add bridge env vars
        cmd.env("BENTOMUX_PANE_ID", &id);
        
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Shell spawn failed: {}", e))?;

        let pid = child.process_id().unwrap_or(0);
        let writer = pair.master.take_writer().unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();

        let term = Term {
            id: id.clone(),
            workspace_id: workspace_id.clone(),
            pid,
            alive: true,
            writer,
        };

        self.terms.lock().unwrap().insert(id.clone(), term);

        // Spawn data reading task
        let term_id = id.clone();
        let app_handle = self.app_handle.clone();
        let terms = self.terms.clone();
        
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        app_handle
                            .emit_all("pty:data", (term_id.clone(), data))
                            .ok();
                    }
                    Err(_) => break,
                }
            }
            
            // Mark as dead and notify
            if let Some(term) = terms.lock().unwrap().get_mut(&term_id) {
                term.alive = false;
            }
            app_handle.emit_all("pty:exit", (term_id.clone(), 0)).ok();
        });

        Ok(id)
    }

    pub fn write_term(&self, id: &str, data: &str) -> Result<(), String> {
        let mut terms = self.terms.lock().unwrap();
        if let Some(term) = terms.get_mut(id) {
            term.writer
                .write_all(data.as_bytes())
                .map_err(|e| format!("Write failed: {}", e))?;
            term.writer.flush().ok();
            Ok(())
        } else {
            Err("Terminal not found".into())
        }
    }

    pub fn resize_term(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        // portable-pty doesn't expose resize on the writer directly
        // Need to store the master handle separately
        // TODO: Refactor Term struct to hold master instead of just writer
        Ok(())
    }

    pub fn kill_term(&self, id: &str) {
        self.terms.lock().unwrap().remove(id);
    }

    pub fn list_terms(&self) -> Vec<String> {
        self.terms.lock().unwrap().keys().cloned().collect()
    }
}
```

**Implementation Notes:**
- `portable-pty` API differs from node-pty but covers same features
- Resize needs refactoring: store `PtyPair` master instead of just writer
- Data streaming uses tokio task + Tauri event emission
- Error handling: Rust requires explicit error types (use `Result<T, String>`)


### 5. Process Detection & Agent Runtime (runtime.ts → runtime.rs)

**Current (Electron):**
```typescript
// src/main/runtime.ts
import psList from 'ps-list';

async function detectAgents() {
  const procs = await psList();
  for (const term of liveTerms()) {
    const match = findAgentInTree(procs, term.pid);
    // ... evaluate manifests against screen buffer
  }
}
```

**Target (Tauri):**
```rust
// src-tauri/src/runtime.rs
use sysinfo::{System, SystemExt, ProcessExt};
use std::collections::HashMap;
use tauri::{AppHandle, Manager};
use std::time::Duration;

pub struct RuntimeDetector {
    system: System,
    app_handle: AppHandle,
}

impl RuntimeDetector {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            system: System::new_all(),
            app_handle,
        }
    }

    pub fn start_polling(&mut self) {
        let app_handle = self.app_handle.clone();
        
        tokio::spawn(async move {
            let mut system = System::new_all();
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            
            loop {
                interval.tick().await;
                system.refresh_processes();
                
                let statuses = detect_runtime_statuses(&system);
                app_handle.emit_all("rt:status", statuses).ok();
            }
        });
    }
}

fn detect_runtime_statuses(system: &System) -> HashMap<String, RuntimeStatus> {
    let mut statuses = HashMap::new();
    
    // Get all terminal PIDs from PtyManager
    // For each terminal, walk process tree and match agent patterns
    
    for process in system.processes().values() {
        if let Some(agent_name) = match_agent_process(process) {
            // Found an agent process
            // Determine state from screen buffer or activity
            let state = determine_agent_state(process.pid(), agent_name);
            // Map back to terminal pane ID
            // statuses.insert(pane_id, RuntimeStatus { agent: agent_name, state });
        }
    }
    
    statuses
}

fn match_agent_process(process: &sysinfo::Process) -> Option<String> {
    let name = process.name().to_lowercase();
    let cmd = process.cmd().join(" ").to_lowercase();
    
    if name.contains("claude") || cmd.contains("claude-code") {
        return Some("claude".to_string());
    }
    if name.contains("pi-agent") || cmd.contains("pi-coding-agent") {
        return Some("pi".to_string());
    }
    if name.contains("codex") || cmd.contains("@openai/codex") {
        return Some("codex".to_string());
    }
    // ... other agents
    
    None
}
```

### 6. Screen Buffer Parsing (detect/screen.ts → detect/screen.rs)

**Option A: Hybrid Approach (Recommended for MVP)**
```rust
// src-tauri/src/detect/screen.rs
use std::process::{Command, Stdio};
use std::io::Write;

// Ship a minimal Node.js script with the app that uses @xterm/headless
// Communicate via stdin/stdout

pub fn parse_screen_buffer(pty_output: &str) -> Result<Vec<String>, String> {
    let node_script = include_str!("../../resources/screen-parser.js");
    
    let mut child = Command::new("node")
        .arg("-e")
        .arg(node_script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Node spawn failed: {}", e))?;
    
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(pty_output.as_bytes()).ok();
    }
    
    let output = child.wait_with_output()
        .map_err(|e| format!("Node process failed: {}", e))?;
    
    let lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();
    
    Ok(lines)
}
```

**Option B: Pure Rust (Future Enhancement)**
```rust
// src-tauri/src/detect/screen.rs
use vt100::Parser;

pub struct ScreenParser {
    parser: Parser,
}

impl ScreenParser {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: Parser::new(rows, cols, 0),
        }
    }
    
    pub fn process(&mut self, data: &[u8]) {
        self.parser.process(data);
    }
    
    pub fn get_lines(&self) -> Vec<String> {
        let screen = self.parser.screen();
        (0..screen.rows())
            .map(|row| screen.row_contents(row).trim().to_string())
            .collect()
    }
}
```

### 7. Agent Approval Bridge (bridge.ts → bridge.rs)

**Current (Electron):**
```typescript
// src/main/bridge.ts
import * as net from 'net';

let server: net.Server;

export function startBridge() {
  const addr = process.platform === 'win32' 
    ? '\\\\.\\pipe\\bentomux-bridge'
    : '/tmp/bentomux-bridge.sock';
  
  server = net.createServer((socket) => {
    let buf = '';
    socket.on('data', (chunk) => {
      buf += chunk.toString();
      const lines = buf.split('\n');
      buf = lines.pop() || '';
      for (const line of lines) {
        handleEnvelope(JSON.parse(line), socket);
      }
    });
  });
  
  server.listen(addr);
}
```

**Target (Tauri - Unix):**
```rust
// src-tauri/src/bridge.rs
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct HookEnvelope {
    event: String,
    pane: Option<String>,
    payload: serde_json::Value,
}

pub struct BridgeServer {
    pending: Arc<Mutex<HashMap<String, (AgentApproval, UnixStream)>>>,
    app_handle: AppHandle,
}

impl BridgeServer {
    pub async fn start(app_handle: AppHandle) -> Result<Self, String> {
        let socket_path = "/tmp/bentomux-bridge.sock";
        
        // Remove old socket if exists
        let _ = std::fs::remove_file(socket_path);
        
        let listener = UnixListener::bind(socket_path)
            .map_err(|e| format!("Socket bind failed: {}", e))?;
        
        let server = Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            app_handle: app_handle.clone(),
        };
        
        let pending = server.pending.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let app = app_handle.clone();
                        let pend = pending.clone();
                        tokio::spawn(handle_connection(stream, app, pend));
                    }
                    Err(e) => eprintln!("Accept error: {}", e),
                }
            }
        });
        
        Ok(server)
    }
    
    pub async fn resolve_approval(&self, request_id: String, approved: bool) {
        let mut pending = self.pending.lock().await;
        if let Some((_, mut stream)) = pending.remove(&request_id) {
            let response = serde_json::json!({
                "directive": if approved { "approve" } else { "deny" }
            });
            stream.write_all(response.to_string().as_bytes()).await.ok();
            stream.write_all(b"\n").await.ok();
        }
    }
}

async fn handle_connection(
    stream: UnixStream,
    app_handle: AppHandle,
    pending: Arc<Mutex<HashMap<String, (AgentApproval, UnixStream)>>>,
) {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    
    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(envelope) = serde_json::from_str::<HookEnvelope>(&line) {
            if envelope.event == "PermissionRequest" {
                let request_id = format!("req-{}", chrono::Utc::now().timestamp_millis());
                let approval = AgentApproval {
                    id: request_id.clone(),
                    pane: envelope.pane,
                    message: envelope.payload["message"].as_str().unwrap_or("").to_string(),
                };
                
                // Store connection
                pending.lock().await.insert(request_id.clone(), (approval.clone(), stream.try_clone().unwrap()));
                
                // Emit to frontend
                app_handle.emit_all("agent:approval", approval).ok();
            }
        }
    }
}
```

**Target (Tauri - Windows Named Pipes):**
```rust
// Windows version using windows-sys crate
// Similar pattern but using CreateNamedPipeW / ConnectNamedPipe
// Implementation deferred - complex unsafe code
```

### 8. Remote Monitor Server (remote.ts → remote.rs)

```rust
// src-tauri/src/remote.rs
use axum::{Router, routing::get, extract::State, response::Html};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use qrcode::QrCode;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct RemoteServer {
    port: u16,
    token: String,
}

impl RemoteServer {
    pub async fn start(port: u16, token: String, app_handle: AppHandle) -> Result<Self, String> {
        let app_state = Arc::new(RemoteState {
            token: token.clone(),
            app_handle,
        });
        
        let app = Router::new()
            .route("/", get(pairing_page))
            .route("/ws", get(websocket_handler))
            .with_state(app_state);
        
        let addr = format!("0.0.0.0:{}", port);
        tokio::spawn(async move {
            axum::Server::bind(&addr.parse().unwrap())
                .serve(app.into_make_service())
                .await
                .ok();
        });
        
        Ok(Self { port, token })
    }
    
    pub fn generate_qr(&self) -> Result<String, String> {
        let local_ip = get_local_ip()?;
        let url = format!("http://{}:{}/?token={}", local_ip, self.port, self.token);
        
        let code = QrCode::new(url.as_bytes())
            .map_err(|e| format!("QR generation failed: {}", e))?;
        
        // Convert to SVG or PNG base64
        Ok(code.render::<char>().build())
    }
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RemoteState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

async fn handle_websocket(socket: WebSocket, state: Arc<RemoteState>) {
    // Stream terminal output to connected clients
    // Handle approval decisions from remote
}
```

---

## Main Entry Point Integration

```rust
// src-tauri/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod state;
mod commands;
mod pty;
mod git;
mod runtime;
mod bridge;
mod remote;
mod agents;
mod detect;

use state::AppStateManager;
use pty::PtyManager;
use runtime::RuntimeDetector;

#[tokio::main]
async fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle();
            
            // Initialize state
            let state_manager = AppStateManager::new(&app_handle);
            let pty_manager = PtyManager::new(app_handle.clone());
            
            // Start background services
            let mut runtime_detector = RuntimeDetector::new(app_handle.clone());
            runtime_detector.start_polling();
            
            // Start bridge if configured
            tokio::spawn(async move {
                if let Err(e) = bridge::BridgeServer::start(app_handle.clone()).await {
                    eprintln!("Bridge start failed: {}", e);
                }
            });
            
            // Start remote monitor if enabled
            let state = state_manager.get_state();
            if let Some(remote_prefs) = state.prefs.remote {
                if remote_prefs.enabled.unwrap_or(false) {
                    let port = remote_prefs.port.unwrap_or(8765);
                    let token = remote_prefs.token.unwrap_or_else(|| "".to_string());
                    tokio::spawn(async move {
                        remote::RemoteServer::start(port, token, app_handle.clone()).await.ok();
                    });
                }
            }
            
            // Register state managers
            app.manage(state_manager);
            app.manage(pty_manager);
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::workspace_add,
            commands::workspace_remove,
            commands::tab_create,
            commands::tab_close,
            commands::tab_rename,
            commands::pty_write,
            commands::pty_resize,
            commands::git_status,
            commands::git_diff,
            commands::git_push,
            commands::prefs_update,
            // ... all other commands
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

---

## Agent Config Adapters Migration

### Directory Structure
```
src-tauri/src/agents/
├── mod.rs              # Public interface
├── claude.rs           # Claude Code adapter
├── pi.rs               # pi agent adapter
├── codex.rs            # OpenAI Codex adapter
├── gemini.rs           # Gemini CLI adapter
├── opencode.rs         # OpenCode adapter
├── qwen.rs             # Qwen CLI adapter
├── cursor.rs           # Cursor adapter
└── traits.rs           # Shared adapter trait
```

### Agent Adapter Trait
```rust
// src-tauri/src/agents/traits.rs
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct ModelSettings {
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub context: Option<u32>,
    pub api_key: Option<String>,
    pub format: Option<String>,
}

pub trait AgentAdapter {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn config_path(&self) -> Option<PathBuf>;
    fn detect(&self) -> bool;
    fn read_model_settings(&self) -> Result<ModelSettings, String>;
    fn write_model_settings(&self, settings: ModelSettings) -> Result<(), String>;
    fn list_resources(&self, kind: &str) -> Result<Vec<String>, String>;
    fn toggle_resource(&self, kind: &str, name: &str, enabled: bool) -> Result<(), String>;
}
```

### Example: Claude Code Adapter
```rust
// src-tauri/src/agents/claude.rs
use super::traits::{AgentAdapter, ModelSettings};
use serde_yaml;
use std::path::PathBuf;
use std::fs;
use dirs;

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn id(&self) -> &str { "claude" }
    fn name(&self) -> &str { "Claude Code" }
    
    fn config_path(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude").join("config.yaml"))
    }
    
    fn detect(&self) -> bool {
        self.config_path().map(|p| p.exists()).unwrap_or(false)
    }
    
    fn read_model_settings(&self) -> Result<ModelSettings, String> {
        let path = self.config_path().ok_or("Config path not found")?;
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Read failed: {}", e))?;
        
        let yaml: serde_yaml::Value = serde_yaml::from_str(&content)
            .map_err(|e| format!("YAML parse failed: {}", e))?;
        
        Ok(ModelSettings {
            model: yaml["model"].as_str().map(String::from),
            base_url: yaml["baseUrl"].as_str().map(String::from),
            context: yaml["context"].as_u64().map(|n| n as u32),
            api_key: None, // Never read keys
            format: None,
        })
    }
    
    fn write_model_settings(&self, settings: ModelSettings) -> Result<(), String> {
        let path = self.config_path().ok_or("Config path not found")?;
        let mut yaml: serde_yaml::Value = if path.exists() {
            let content = fs::read_to_string(&path).unwrap_or_default();
            serde_yaml::from_str(&content).unwrap_or(serde_yaml::Value::Mapping(Default::default()))
        } else {
            serde_yaml::Value::Mapping(Default::default())
        };
        
        if let Some(model) = settings.model {
            yaml["model"] = serde_yaml::Value::String(model);
        }
        if let Some(base_url) = settings.base_url {
            yaml["baseUrl"] = serde_yaml::Value::String(base_url);
        }
        if let Some(context) = settings.context {
            yaml["context"] = serde_yaml::Value::Number(context.into());
        }
        
        let content = serde_yaml::to_string(&yaml)
            .map_err(|e| format!("YAML serialize failed: {}", e))?;
        
        fs::write(&path, content)
            .map_err(|e| format!("Write failed: {}", e))?;
        
        Ok(())
    }
    
    fn list_resources(&self, kind: &str) -> Result<Vec<String>, String> {
        // Read memory/skills/mcp directories
        let base = dirs::home_dir().ok_or("Home dir not found")?;
        let dir = match kind {
            "memory" => base.join(".claude").join("memory"),
            "skills" => base.join(".claude").join("skills"),
            "mcp" => return Ok(vec![]), // MCP in config.yaml
            _ => return Err("Unknown resource kind".into()),
        };
        
        if !dir.exists() {
            return Ok(vec![]);
        }
        
        let entries = fs::read_dir(dir)
            .map_err(|e| format!("Read dir failed: {}", e))?;
        
        let names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .collect();
        
        Ok(names)
    }
    
    fn toggle_resource(&self, kind: &str, name: &str, enabled: bool) -> Result<(), String> {
        // Implementation depends on agent's native toggle mechanism
        // For Claude: rename file.md <-> file.md.disabled
        let base = dirs::home_dir().ok_or("Home dir not found")?;
        let dir = match kind {
            "memory" => base.join(".claude").join("memory"),
            "skills" => base.join(".claude").join("skills"),
            _ => return Err("Unknown resource kind".into()),
        };
        
        let active_path = dir.join(format!("{}.md", name));
        let disabled_path = dir.join(format!("{}.md.disabled", name));
        
        if enabled {
            if disabled_path.exists() {
                fs::rename(&disabled_path, &active_path)
                    .map_err(|e| format!("Rename failed: {}", e))?;
            }
        } else {
            if active_path.exists() {
                fs::rename(&active_path, &disabled_path)
                    .map_err(|e| format!("Rename failed: {}", e))?;
            }
        }
        
        Ok(())
    }
}
```

**Migration Notes:**
- Port each of the 9 agent adapters from TypeScript to Rust
- Pattern is mechanical: read YAML/JSON/TOML, parse, modify, write
- Use `serde_yaml`, `serde_json`, `toml` crates
- File operations: `std::fs` for sync I/O (adapter methods are sync)

---

## Testing Migration

### Current Test Scripts (Node.js)
```
scripts/
├── test-adapters.ts      # Agent config read/write
├── test-detect.ts        # Agent process detection
├── test-splits.ts        # Split-tree operations
├── test-tabtitle.ts      # Tab key/title resolution
├── test-git-ops.ts       # Git CLI wrappers
├── test-agent-hooks.ts   # Hook bridge protocol
└── test-shiki-tokenize.ts # Syntax highlighting
```

### Target Test Structure (Rust)
```
src-tauri/src/
├── state.rs
│   └── #[cfg(test)] mod tests { ... }
├── pty.rs
│   └── #[cfg(test)] mod tests { ... }
├── git.rs
│   └── #[cfg(test)] mod tests { ... }
└── tests/
    ├── integration/
    │   ├── split_tree.rs
    │   ├── agent_adapters.rs
    │   └── git_ops.rs
    └── common/
        └── fixtures.rs
```

### Example: Split-Tree Test Migration

**Current (TypeScript):**
```typescript
// scripts/test-splits.ts
const tree = splitLeaf(leafNode('a'), 'a', 'h', 'b', 'k1');
assert(tree.kind === 'split');
assert(tree.dir === 'h');
```

**Target (Rust):**
```rust
// src-tauri/src/split_tree.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_leaf_horizontal() {
        let tree = split_leaf(leaf_node("a"), "a", Direction::Horizontal, "b", "k1");
        assert!(matches!(tree.kind, NodeKind::Split));
        assert_eq!(tree.dir, Direction::Horizontal);
    }

    #[test]
    fn test_split_tree_restore() {
        let a = leaf_node("a");
        let b = leaf_node("b");
        let c = leaf_node("c");
        
        let tree = split_leaf(a, "a", Direction::Horizontal, "b", "k1");
        let tree = split_leaf(tree, "b", Direction::Vertical, "c", "k2");
        
        // Serialize + deserialize
        let json = serde_json::to_string(&tree).unwrap();
        let restored: PaneNode = serde_json::from_str(&json).unwrap();
        
        assert_eq!(leaf_ids(&tree), leaf_ids(&restored));
    }
}
```

### Integration Test: Agent Adapters
```rust
// src-tauri/tests/integration/agent_adapters.rs
use bentomux::agents::{ClaudeAdapter, AgentAdapter};
use std::env;

#[test]
fn test_claude_adapter_read_write() {
    // Set up temp config
    let temp_dir = env::temp_dir().join("bentomux-test");
    std::fs::create_dir_all(&temp_dir).unwrap();
    
    let adapter = ClaudeAdapter;
    
    // Write settings
    let settings = ModelSettings {
        model: Some("claude-sonnet-4".to_string()),
        base_url: Some("https://api.anthropic.com".to_string()),
        context: Some(200000),
        api_key: None,
        format: None,
    };
    
    adapter.write_model_settings(settings.clone()).unwrap();
    
    // Read back
    let read = adapter.read_model_settings().unwrap();
    assert_eq!(read.model, settings.model);
    assert_eq!(read.base_url, settings.base_url);
    
    // Cleanup
    std::fs::remove_dir_all(&temp_dir).ok();
}
```

### Running Tests
```bash
# Unit tests (all #[test] functions)
cargo test

# Integration tests only
cargo test --test '*'

# Specific test
cargo test test_split_tree_restore

# With output
cargo test -- --nocapture
```

---

## Renderer Migration (Minimal Changes)

### Changes Required in Renderer

1. **Remove Electron preload bridge**
```typescript
// BEFORE (Electron)
declare global {
  interface Window {
    bentomux: BentomuxApi;
  }
}

// Access via: window.bentomux.workspaceAdd(path)
```

2. **Add Tauri invoke wrapper**
```typescript
// AFTER (Tauri)
// src/renderer/src/api.ts - NEW FILE
import { invoke } from '@tauri-apps/api/core';

export const api = {
  workspaceAdd: (path: string) => invoke('workspace_add', { path }),
  tabCreate: (workspaceId: string) => invoke('tab_create', { workspaceId }),
  ptyWrite: (id: string, data: string) => invoke('pty_write', { id, data }),
  // ... all other commands
};
```

3. **Replace IPC calls throughout renderer**
```typescript
// BEFORE
const state = await window.bentomux.workspaceAdd(path);

// AFTER
import { api } from './api';
const state = await api.workspaceAdd(path);
```

4. **Replace event listeners**
```typescript
// BEFORE (Electron)
window.bentomux.onPtyData((id, data) => {
  terminal.write(data);
});

// AFTER (Tauri)
import { listen } from '@tauri-apps/api/event';

await listen('pty:data', (event) => {
  const [id, data] = event.payload as [string, string];
  terminal.write(data);
});
```

### Search and Replace Operations

Run these in `src/renderer/` to migrate IPC calls:

```bash
# Replace window.bentomux calls
find src/renderer -name "*.ts" -exec sed -i '' 's/window\.bentomux\./api./g' {} +

# Add api import where needed
# (Manual: add `import { api } from './api'` to files using it)
```

---

## Build System Changes

### package.json Updates
```json
{
  "scripts": {
    "dev": "tauri dev",
    "build": "tauri build",
    "tauri": "tauri",
    "preview": "vite preview"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.1.0",
    "electron": "REMOVE",
    "electron-builder": "REMOVE",
    "electron-vite": "REMOVE"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.1.0"
  }
}
```

### Vite Config for Tauri
```typescript
// vite.config.ts - NEW FILE
import { defineConfig } from 'vite';
import { resolve } from 'path';

export default defineConfig({
  root: 'src/renderer',
  build: {
    outDir: '../../out/renderer',
    emptyOutDir: true,
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src/renderer/src'),
    },
  },
});
```

### CI/CD GitHub Actions
```yaml
# .github/workflows/build.yml
name: Build Tauri App

on:
  push:
    branches: [main]
  pull_request:

jobs:
  build:
    strategy:
      matrix:
        os: [windows-latest, macos-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
      
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Install dependencies
        run: npm ci
      
      - name: Build Tauri app
        run: npm run tauri build
      
      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: bentomux-${{ matrix.os }}
          path: src-tauri/target/release/bundle/
```

---

## Step-by-Step Migration Execution Plan

### Phase 1: Project Setup (Day 1)

**1.1 Initialize Tauri**
```bash
cd bentomux
npm install --save-dev @tauri-apps/cli
npm install @tauri-apps/api
npm run tauri init
```

**1.2 Create Rust source structure**
```bash
cd src-tauri/src
touch state.rs commands.rs pty.rs git.rs runtime.rs bridge.rs remote.rs
mkdir agents detect
touch agents/mod.rs agents/traits.rs agents/claude.rs
touch detect/mod.rs detect/screen.rs
```

**1.3 Update Cargo.toml with dependencies**
- Copy the dependencies section from this spec
- Run `cargo check` to download and verify

**1.4 Configure tauri.conf.json**
- Set window size, icon paths
- Configure bundle targets (nsis, msi)
- Add resources path for bentomux-hook.cjs

**Checkpoint:** `cargo build` completes without errors (even with empty modules)

---

### Phase 2: Core State & Types (Days 2-3)

**2.1 Port split-tree logic**
```bash
# Create src-tauri/src/split_tree.rs
# Port from src/shared/split-tree.ts
# Add Serialize/Deserialize derives
# Write unit tests
```

**2.2 Port type definitions**
- Create all struct definitions in `state.rs`
- Match field names from TypeScript (use snake_case in Rust, serde handles conversion)
- Add `#[derive(Serialize, Deserialize, Clone)]` to all types

**2.3 Implement AppStateManager**
- File I/O (read/write bentomux.json)
- Mutex-wrapped state
- Update methods

**2.4 Write state persistence tests**
```bash
cargo test state
```

**Checkpoint:** State loads from JSON, updates persist correctly

---

### Phase 3: PTY Management (Days 4-6)

**3.1 Implement basic PTY spawn**
```rust
// Start with fixed shell (pwsh/bash)
// Get basic spawn + data streaming working
```

**3.2 Add shell detection**
- Port `shell-detect.ts` logic
- Windows: check for pwsh, powershell, cmd, git-bash
- Unix: check for bash, zsh, fish

**3.3 Implement PTY data streaming**
- Tokio async reader task
- Emit `pty:data` events to frontend
- Buffer handling (8KB chunks)

**3.4 Add PTY lifecycle management**
- Resize support
- Write support
- Exit handling + cleanup
- Kill operations

**3.5 Test with real terminal**
```bash
cargo run
# Create terminal pane
# Type commands, verify echo
# Resize window, verify cols/rows update
```

**Checkpoint:** Can spawn shell, type commands, see output in xterm.js

---

### Phase 4: IPC Commands (Days 7-9)

**4.1 Implement P0 commands**
```rust
// commands.rs
workspace_add
workspace_remove
tab_create
tab_close
pty_write
pty_resize
get_state
```

**4.2 Update renderer to use Tauri invoke**
- Create `src/renderer/src/api.ts`
- Replace all `window.bentomux.*` calls
- Update event listeners to use `@tauri-apps/api/event`

**4.3 Wire up main.rs**
- Register all commands in `invoke_handler`
- Add state managers to app context
- Test each command from frontend

**Checkpoint:** Can add workspace, create tabs, type in terminals

---

### Phase 5: Git Operations (Days 10-11)

**5.1 Implement git CLI wrappers**
```rust
// git.rs
git_status -> parse porcelain output
git_diff -> return raw diff
git_push -> run with -u origin HEAD
```

**5.2 Add file watching**
```rust
// git_watcher.rs
// Watch .git/HEAD and .git/refs
// Emit branch events on change
```

**5.3 Test git operations**
- Make changes in a workspace
- Verify changes pill updates
- Open git panel, see file list
- Click file, see diff with syntax highlighting

**Checkpoint:** Git panel shows correct status, diffs render

---

### Phase 6: Agent Detection (Days 12-15)

**6.1 Implement process table polling**
```rust
// runtime.rs
// Use sysinfo crate
// Poll every 2 seconds
// Match agent process patterns
```

**6.2 Port agent detection rules**
- Copy regex patterns from `runtime.ts`
- Match process names and command lines
- Build process tree walker

**6.3 Implement screen buffer parsing**
- **Option A (MVP):** Ship Node.js subprocess using @xterm/headless
- **Option B (Future):** Pure Rust with vt100 crate

**6.4 Port manifest evaluation**
- Copy manifest rules from `detect/manifests.ts`
- Evaluate against screen lines
- Determine idle/working/blocked state

**6.5 Test with live agent**
```bash
# Run claude or pi in a terminal
# Verify runtime status pill shows correct state
```

**Checkpoint:** Agent detection works, status updates in real-time

---

### Phase 7: Agent Config Adapters (Days 16-18)

**7.1 Implement adapter trait**
```rust
// agents/traits.rs
```

**7.2 Port all 9 agent adapters**
- Claude Code (YAML)
- pi (YAML)
- Codex (JSON)
- Gemini (JSON)
- OpenCode (YAML)
- Qwen (YAML)
- Cursor (JSON)
- Kilo (YAML)
- QwenPaw (YAML)

**7.3 Implement config UI commands**
```rust
agents_list
agents_config
agents_update_model
agents_toggle_resource
```

**7.4 Test config read/write**
- Open agent settings modal
- Change model, base URL
- Save, verify config file updated
- Toggle memory/skill, verify file renamed

**Checkpoint:** All agent adapters work, config writes persist

---

### Phase 8: Approval Bridge (Days 19-22)

**8.1 Implement Unix socket server**
```rust
// bridge.rs (Unix)
// Create /tmp/bentomux-bridge.sock
// Accept connections, parse JSON envelopes
```

**8.2 Implement Windows named pipe server**
```rust
// bridge.rs (Windows)
// Create \\.\pipe\bentomux-bridge
// Use windows-sys crate
```

**8.3 Add approval lifecycle**
- Store pending approvals
- Emit events to frontend
- Handle approve/deny decisions
- Send directive back to agent

**8.4 Update bentomux-hook.cjs paths**
- Adjust socket path resolution for Tauri
- Test hook CLI standalone

**8.5 Test with agent hook**
```bash
# Configure agent to use bentomux hook
# Trigger permission request
# Verify overlay shows, approve works
```

**Checkpoint:** Approval bridge works, agent receives decisions

---

### Phase 9: Remote Monitor (Days 23-25)

**9.1 Implement HTTP server**
```rust
// remote.rs
// Use axum, serve pairing page
```

**9.2 Implement WebSocket streaming**
- Accept WS connections
- Stream terminal output
- Handle approval decisions from remote

**9.3 Add QR code generation**
```rust
// Use qrcode crate
// Generate pairing URL with token
```

**9.4 Test remote access**
- Enable remote in settings
- Scan QR code on phone
- Verify pane list shows
- Watch terminal output
- Approve agent from phone

**Checkpoint:** Remote monitor works from phone browser

---

### Phase 10: Testing & Validation (Days 26-30)

**10.1 Port all test scripts to Rust tests**
```bash
cargo test --all
```

**10.2 Manual testing checklist**
- Walk through every feature in UI
- Test on Windows, macOS, Linux
- Verify all keyboard shortcuts
- Test edge cases (terminal exits, git errors, etc.)

**10.3 Performance testing**
- Spawn 10+ terminals
- Check memory usage
- Verify no leaks over 1 hour

**10.4 Build installers**
```bash
npm run tauri build
```

**10.5 Smoke test installers**
- Install on clean VM
- Verify all features work
- Check file size

**Checkpoint:** All tests pass, installers work, size target met

---

## Troubleshooting Guide

### PTY Issues

**Problem:** Terminal spawns but no output
```rust
// Check: reader task is running
tokio::spawn(async move {
    println!("Reader task started for {}", term_id); // Add logging
    // ...
});
```

**Problem:** Output garbled or missing characters
```rust
// Check: buffer size adequate, UTF-8 handling
let mut buf = [0u8; 8192]; // Increase if needed
let data = String::from_utf8_lossy(&buf[..n]); // Use lossy conversion
```

**Problem:** Windows ConPTY fails to spawn
```rust
// Check: ConPTY binaries available
// portable-pty should auto-detect, but verify Windows version >= 1809
```

### IPC Issues

**Problem:** Command not found
```rust
// Verify command registered in main.rs invoke_handler
.invoke_handler(tauri::generate_handler![
    commands::workspace_add, // Must be listed here
])
```

**Problem:** Serialization error
```rust
// Check: all types have Serialize/Deserialize
#[derive(Serialize, Deserialize, Clone)]
pub struct MyType { ... }
```

### Build Issues

**Problem:** Linker errors on Windows
```bash
# Install Visual Studio Build Tools
# Ensure MSVC toolchain selected
rustup default stable-msvc
```

**Problem:** node-pty reference errors
```bash
# Remove node-pty from package.json dependencies
# It's Electron-specific, not needed in Tauri
```

**Problem:** Missing webview runtime
```bash
# Windows: Install Edge WebView2 Runtime
# Linux: Install webkit2gtk
sudo apt install libwebkit2gtk-4.0-dev
```

---

## Current Verification Evidence

The following checks are verified in the Tauri v2 port:

- Rust test suite: 88 tests passed.
- Frontend typecheck and Vite production build: passed.
- macOS `Bentomux.app`: built successfully; app bundle measured at 7.6 MB.
- macOS DMG: `Bentomux_0.1.0_x64.dmg` built successfully; measured at 4,248,074 bytes.
- Bundled resources: `bentomux-hook.cjs`, `remote-page.html`, and agent manifests were present in the app bundle before DMG packaging.
- Backend startup readiness: release binary samples 1,738 ms, 1,640 ms, and 1,281 ms; median 1,640 ms.
- Release-process RSS sample: 96,916 KB (approximately 94.6 MB) at idle after launch; this exceeds the 50–80 MB target and is one sample, not a leak result.

Still requiring platform evidence before marking this migration complete:

- Windows named-pipe approval lifecycle and Windows MSI.
- Linux AppImage and Linux runtime smoke test.
- Native end-to-end workspace, terminal, git, remote, and approval flow.
- Release idle-memory and one-hour leak measurements.
- Final cross-platform verification matrix.

---


## Validation Checklist
### Core Features
- [ ] Launch app, main window appears
- [ ] Add workspace, folder appears in sidebar
- [ ] Create terminal tab, shell prompt appears
- [ ] Type commands, see output in real-time
- [ ] Split terminal (right/down), both panes work
- [ ] Resize window, terminals adjust
- [ ] Close pane, layout updates correctly
- [ ] Restart app, tabs/layout restored
- [ ] Rename tab, title persists
- [ ] Copy/paste in terminal works

### Git Features
- [ ] Make file changes, changes pill updates (+N −N)
- [ ] Open git panel, see file list
- [ ] Click file, diff view appears
- [ ] Syntax highlighting in diff correct
- [ ] Push button works, remote updated
- [ ] Switch branch (external tool), pill updates

### Agent Features
- [ ] Run agent in terminal, runtime status shows
- [ ] Agent state detection (idle/working/blocked) works
- [ ] Open agent settings, detect all installed agents
- [ ] Change model setting, config file updates
- [ ] Toggle memory file, file renamed correctly
- [ ] Agent approval request appears
- [ ] Approve button sends decision to agent
- [ ] Agent continues after approval

### Remote Monitor
- [ ] Enable remote, QR code appears
- [ ] Scan QR on phone, pairing page loads
- [ ] Pane list shows all terminals
- [ ] Select pane, output streams
- [ ] Approve agent from phone, works

### Settings
- [ ] Change theme, UI updates
- [ ] Change palette, colors update
- [ ] Change font, terminals update
- [ ] Change shell, new tabs use it
- [ ] Rebind shortcut, new key works
- [ ] All settings persist across restart

### Performance
- [ ] Startup < 2 seconds
- [ ] Memory usage < 150 MB idle
- [ ] No memory leaks over 1 hour
- [ ] Terminal output smooth (no lag)
- [ ] Process polling doesn't spike CPU

### Cross-Platform
- [ ] Windows: installer works, all features function
- [ ] macOS: DMG works, all features function
- [ ] Linux: AppImage works, all features function

### Size Targets
- [ ] Windows installer < 15 MB
- [ ] macOS DMG < 12 MB
- [ ] Linux AppImage < 20 MB
- [ ] Unpacked app < 50 MB

---

## Post-Migration Cleanup

### Remove Electron Dependencies
```bash
npm uninstall electron electron-builder electron-vite @electron/rebuild
rm -rf src/preload
```

### Update Documentation
- Update README.md with new build commands
- Update CONTRIBUTING.md with Rust setup
- Add Rust code style guide

### Archive Old Code
```bash
mkdir _electron-legacy
mv src/main _electron-legacy/
mv src/preload _electron-legacy/
```

---

## Success Metrics

| Metric | Before (Electron) | Target (Tauri) | Achieved |
|--------|-------------------|----------------|----------|
| Installer Size | 106 MB | 8-10 MB | _____ MB |
| Unpacked Size | 409 MB | 15-30 MB | _____ MB |
| Cold Start | 2-3s | 0.5-1s | _____ s |
| Idle Memory | 250 MB | 50-80 MB | _____ MB |
| Feature Parity | 100% | 100% | _____ % |

---

## Migration Complete!

When all checkpoints pass and validation checklist is complete, you have successfully migrated Bentomux from Electron to Tauri with:

✅ **95% size reduction** (106 MB → 8-10 MB)  
✅ **3x faster startup** (2-3s → 0.5-1s)  
✅ **70% less memory** (250 MB → 80 MB)  
✅ **100% feature parity** (zero regressions)  
✅ **Native performance** (Rust backend)  
✅ **Modern architecture** (Tauri 2.x + Tokio async)

**Next Steps:**
1. Ship beta to users for feedback
2. Monitor for edge cases over 2-4 weeks
3. Deprecate Electron version after stabilization
4. Consider pure-Rust screen parser to eliminate Node.js subprocess

---

**END OF MIGRATION SPECIFICATION**
