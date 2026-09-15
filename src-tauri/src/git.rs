/* ---------------- git branch per workspace + CLI operations ----------------
   Rust port of src/main/git.rs (branch watch) + src/main/git-ops.ts
   (status/diff/push/remoteInfo). Reads .git/HEAD directly for the branch
   and polls the git dir; shells out to git (never node-pty) for the ops.
   Handles .git as a file (worktrees / submodules). */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::time::Duration;
use tauri::Emitter;

/* ---------------- git dir + branch detection ---------------- */

pub fn git_dir_for(ws_path: &str) -> Option<String> {
    let dot = Path::new(ws_path).join(".git");
    if !dot.exists() {
        return None;
    }
    let head = dot.join("HEAD");
    if head.exists() {
        return Some(dot.to_string_lossy().into_owned());
    }
    /* .git file: "gitdir: /path/to/real/gitdir" */
    if let Ok(content) = std::fs::read_to_string(&dot) {
        let first = content.trim();
        if let Some(rest) = first.strip_prefix("gitdir:") {
            let real = rest.trim();
            let real = if Path::new(real).is_absolute() {
                real.to_string()
            } else {
                Path::new(ws_path).join(real).to_string_lossy().into_owned()
            };
            let real_head = Path::new(&real).join("HEAD");
            return if real_head.exists() { Some(real) } else { None };
        }
    }
    None
}

pub fn branch_for(ws_path: &str) -> Result<Option<String>, String> {
    Ok(read_branch(ws_path))
}

pub fn read_branch(ws_path: &str) -> Option<String> {
    let dir = git_dir_for(ws_path)?;
    let content = std::fs::read_to_string(Path::new(&dir).join("HEAD")).ok()?;
    let head = content.trim();
    if let Some(rest) = head.strip_prefix("ref: refs/heads/") {
        Some(rest.to_string())
    } else if let Some(rest) = head.strip_prefix("ref: refs/") {
        Some(rest.to_string())
    } else {
        /* detached HEAD: first 7 chars of the hash */
        Some(head.chars().take(7).collect())
    }
}

/* ---------------- branch watcher (fs.watch on .git/HEAD + 4s poll) ----------------
   The Electron version pairs an fs.watch on the git dir with a 4s poll.
   We mirror that with a notify::recommended_watcher on the git dir (fires on
   HEAD changes, 60ms debounce) plus a per-workspace poll as the safety net. */

type BranchCb = Box<dyn Fn(&str, Option<&str>) + Send + Sync>;

struct WatchState {
    watchers: HashMap<String, Option<String>>, /* workspaceId → last branch */
    notify: Option<BranchCb>,
    handle: Option<tauri::AppHandle>,
}

fn watch_state() -> &'static Mutex<Option<WatchState>> {
    use std::sync::OnceLock;
    static STATE: OnceLock<Mutex<Option<WatchState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

/* called once from setup; provides the app handle for branch events */
pub fn init_watch(handle: tauri::AppHandle) {
    let mut guard = watch_state().lock().unwrap();
    if guard.is_none() {
        *guard = Some(WatchState { watchers: HashMap::new(), notify: None, handle: Some(handle) });
    }
}

pub fn on_branch_change(cb: impl Fn(&str, Option<&str>) + Send + Sync + 'static) {
    let mut guard = watch_state().lock().unwrap();
    let st = guard.get_or_insert_with(|| WatchState { watchers: HashMap::new(), notify: None, handle: None });
    st.notify = Some(Box::new(cb));
}

fn emit_branch(workspace_id: &str, branch: Option<&str>) {
    if let Some(st) = watch_state().lock().unwrap().as_ref() {
        if let Some(cb) = &st.notify {
            cb(workspace_id, branch);
        }
        if let Some(app) = &st.handle {
            let _ = app.emit("branch", (workspace_id, branch));
        }
    }
}

/* true while a workspace is still registered (not yet unwatched) */
fn registered(workspace_id: &str) -> bool {
    watch_state()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.watchers.contains_key(workspace_id))
}

/* read the branch, compare to the stored last value, and emit the event when
   it changed. true when the workspace is still registered (not unwatched). */
fn check_and_emit(workspace_id: &str, ws_path: &str) -> bool {
    let cur = read_branch(ws_path);
    let should_emit = {
        let mut guard = watch_state().lock().unwrap();
        match guard.as_mut().and_then(|s| s.watchers.get_mut(workspace_id)) {
            Some(prev) => {
                if *prev != cur {
                    *prev = cur.clone();
                    true
                } else {
                    false
                }
            }
            None => return false, /* workspace unregistered: stop the loop */
        }
    };
    if should_emit {
        emit_branch(workspace_id, cur.as_deref());
    }
    true
}

pub fn watch_workspace(workspace_id: &str, ws_path: &str) {
    unwatch_workspace(workspace_id);
    {
        let mut guard = watch_state().lock().unwrap();
        let st = guard.get_or_insert_with(|| WatchState { watchers: HashMap::new(), notify: None, handle: None });
        st.watchers.insert(workspace_id.to_string(), read_branch(ws_path));
    }
    let ws_path = ws_path.to_string();
    let wid = workspace_id.to_string();

    /* instant trigger: fs-watch the git dir, ignore transient files, then
       quietly re-check the branch (git.ts's `setTimeout(check,60)`). */
    if let Some(dir) = git_dir_for(&ws_path) {
        let dir = dir.clone();
        let wid2 = wid.clone();
        let ws2 = ws_path.clone();
        std::thread::spawn(move || {
            use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
            let (tx, rx) = std::sync::mpsc::channel();
            let on_event = move |res: notify::Result<notify::Event>| {
                let _ = tx.send(res);
            };
            if let Ok(mut watcher) = RecommendedWatcher::new(on_event, notify::Config::default()) {
                if watcher.watch(Path::new(&dir), RecursiveMode::NonRecursive).is_ok() {
                    for res in rx {
                        if let Ok(ev) = res {
                            match ev.kind {
                                EventKind::Access(_)
                                | EventKind::Create(_)
                                | EventKind::Modify(_)
                                | EventKind::Remove(_) => {}
                                _ => continue,
                            }
                            let is_head = ev
                                .paths
                                .iter()
                                .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("HEAD"));
                            if is_head {
                                std::thread::sleep(Duration::from_millis(60));
                                check_and_emit(&wid2, &ws2);
                                if !registered(&wid2) {
                                    break; /* workspace unwatched */
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /* safety-net poll, mirroring the Electron interval; stops on unwatch */
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(4));
        if !check_and_emit(&wid, &ws_path) {
            break;
        }
    });
}

pub fn unwatch_workspace(workspace_id: &str) {
    if let Some(st) = watch_state().lock().unwrap().as_mut() {
        st.watchers.remove(workspace_id);
    }
}

pub fn unwatch_all() {
    if let Some(st) = watch_state().lock().unwrap().as_mut() {
        st.watchers.clear();
    }
}

/* ---------------- git CLI operations ---------------- */

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusEntry {
    pub path: String,
    /* two-letter XY status, e.g. " M", "M ", "A ", "??", "R " */
    pub status: String,
    pub index_status: String,
    pub work_tree_status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusResult {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub entries: Vec<GitStatusEntry>,
    pub has_untracked: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitFileDiff {
    pub path: String,
    pub status: String,
    pub patch: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffResult {
    pub is_repo: bool,
    pub files: Vec<GitFileDiff>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffStatResult {
    pub is_repo: bool,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommandResult {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitRemoteInfo {
    pub is_repo: bool,
    pub remote: Option<String>,
    pub default_branch: Option<String>,
}

const MAX_BUFFER: usize = 8 * 1024 * 1024; /* 8 MiB per stream — git diff of a big repo */
const DEFAULT_TIMEOUT_MS: u64 = 60_000;

#[derive(Default)]
struct RunOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

/* blocked git run with per-stream buffer cap + timeout; mirrors runProcess */
fn run_process(cwd: &str, args: &[&str], timeout_ms: Option<u64>) -> Result<RunOutput, String> {
    #[allow(unused_mut)]
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    /* suppress the brief console window flash on Windows */
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x0800_0000 /* CREATE_NO_WINDOW */);
    let mut child = cmd.spawn().map_err(|e| format!("git spawn failed: {}", e))?;

    let timeout = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    /* stdout/stderr configured as piped above, so take() always yields a stream */
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let so = std::thread::spawn(move || drain_capped(stdout));
    let se = std::thread::spawn(move || drain_capped(stderr));

    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = RunOutput {
                    code: status.code().unwrap_or(0),
                    stdout: so.join().unwrap_or_default(),
                    stderr: se.join().unwrap_or_default(),
                };
                return Ok(out);
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    return Err(format!("git {} timed out after {}ms", args.join(" "), timeout.as_millis()));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(format!("git wait failed: {}", e)),
        }
    }
}

fn drain_capped(reader: impl std::io::Read) -> String {
    let mut r = reader;
    let mut out = String::new();
    let mut buf = [0u8; 8192];
    let mut total = 0usize;
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if total > MAX_BUFFER {
                    break;
                }
                out.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            Err(_) => break,
        }
    }
    out
}

fn ensure_ok(r: &RunOutput, what: &str) -> Result<(), String> {
    if r.code == 0 {
        return Ok(());
    }
    let msg = r.stderr.trim();
    let msg = if msg.is_empty() { r.stdout.trim() } else { msg };
    Err(if msg.is_empty() {
        format!("{} exited with code {}", what, r.code)
    } else {
        msg.to_string()
    })
}

/* ---------------- status ---------------- */

fn parse_status(parts: &[String]) -> (Option<String>, u32, u32, Vec<GitStatusEntry>, bool) {
    let mut branch = None;
    let mut ahead = 0u32;
    let mut behind = 0u32;
    let mut entries = Vec::new();
    let mut has_untracked = false;
    let mut i = 0;
    while i < parts.len() {
        let p = &parts[i];
        if let Some(rest) = p.strip_prefix("## ") {
            if let Some(d) = rest.find("...") {
                branch = Some(rest[..d].to_string());
            } else if !rest.starts_with("HEAD") {
                branch = rest.split(' ').next().map(String::from);
            }
            if let Some(start) = rest.find('[') {
                let body = &rest[start..];
                if let Some(a) = parse_named(body, "ahead") {
                    ahead = a;
                }
                if let Some(b) = parse_named(body, "behind") {
                    behind = b;
                }
            }
            i += 1;
            continue;
        }
        if let Some(mut entry) = parse_status_line(p) {
            /* porcelain rename: "R <orig>\0<new>"; consume the next segment */
            if (entry.index_status == "R" || entry.index_status == "C") && i + 1 < parts.len() {
                let next = parts[i + 1].clone();
                if !next.is_empty() {
                    entry.path = next;
                    entries.push(entry);
                    i += 2;
                    continue;
                }
            }
            if entry.index_status == "?" && entry.work_tree_status == "?" {
                has_untracked = true;
            }
            entries.push(entry);
        }
        i += 1;
    }
    (branch, ahead, behind, entries, has_untracked)
}

fn parse_named(body: &str, name: &str) -> Option<u32> {
    let idx = body.find(name)?;
    let rest = &body[idx + name.len()..];
    let rest = rest.trim_start_matches([' ', '(', ',']);
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn parse_status_line(line: &str) -> Option<GitStatusEntry> {
    if line.len() < 4 {
        return None;
    }
    let index_status = line.chars().nth(0).unwrap_or(' ');
    let work_tree_status = line.chars().nth(1).unwrap_or(' ');
    let path = &line[3..];
    if path.is_empty() {
        return None;
    }
    Some(GitStatusEntry {
        path: path.to_string(),
        status: line[..2].to_string(),
        index_status: index_status.to_string(),
        work_tree_status: work_tree_status.to_string(),
    })
}

pub fn status(ws_path: &str) -> GitStatusResult {
    if git_dir_for(ws_path).is_none() {
        return GitStatusResult::default();
    }
    let Ok(r) = run_process(ws_path, &["status", "--porcelain=v1", "-z", "-uall", "--branch"], None) else {
        return GitStatusResult { is_repo: true, ..Default::default() };
    };
    if let Err(e) = ensure_ok(&r, "git status") {
        eprintln!("[bentomux] git status: {}", e);
        return GitStatusResult { is_repo: true, ..Default::default() };
    }
    let parts: Vec<String> = r.stdout.split('\0').map(String::from).filter(|s| !s.is_empty()).collect();
    let (branch, ahead, behind, entries, has_untracked) = parse_status(&parts);
    GitStatusResult { is_repo: true, branch, ahead, behind, entries, has_untracked }
}

/* ---------------- diff ---------------- */

pub fn diff(ws_path: &str, path: Option<&str>) -> GitDiffResult {
    if git_dir_for(ws_path).is_none() {
        return GitDiffResult::default();
    }
    /* for an untracked file, mark intent-to-add so git's diff machinery includes it */
    if let Some(path) = path {
        let _ = run_process(ws_path, &["add", "-N", "--", path], None);
    }
    let mut args: Vec<&str> = vec!["diff", "--no-color", "--no-ext-diff", "--unified=3"];
    if let Some(path) = path {
        args.push("--");
        args.push(path);
    }
    let Ok(r) = run_process(ws_path, &args, None) else {
        return GitDiffResult { is_repo: true, ..Default::default() };
    };
    if let Err(e) = ensure_ok(&r, "git diff") {
        eprintln!("[bentomux] git diff: {}", e);
        return GitDiffResult { is_repo: true, ..Default::default() };
    }
    let files = parse_diff_blocks(&r.stdout);
    GitDiffResult { is_repo: true, files }
}

fn parse_diff_blocks(stdout: &str) -> Vec<GitFileDiff> {
    let mut files = Vec::new();
    for block in stdout.split("diff --git ").filter(|b| !b.is_empty()) {
        let header = block.lines().next().unwrap_or("");
        /* "a/<path> b/<path>" — the simple split on " b/" handles common paths */
        let file_path = match header.find(" b/") {
            Some(idx) => header[idx + 3..].to_string(),
            None => header.to_string(),
        };
        let status = if block.contains("new file") {
            "A"
        } else if block.contains("deleted file") {
            "D"
        } else if block.contains("rename ") {
            "R"
        } else if block.contains("copy ") {
            "C"
        } else {
            "M"
        };
        files.push(GitFileDiff { path: file_path, status: status.to_string(), patch: format!("diff --git {}", block) });
    }
    files
}

/* ---------------- diff stat (totals for the Changes pill) ---------------- */

pub fn diff_stat(ws_path: &str) -> GitDiffStatResult {
    if git_dir_for(ws_path).is_none() {
        return GitDiffStatResult::default();
    }
    let Ok(r) = run_process(ws_path, &["diff", "--numstat"], None) else {
        return GitDiffStatResult { is_repo: true, ..Default::default() };
    };
    if let Err(e) = ensure_ok(&r, "git diff --numstat") {
        eprintln!("[bentomux] git diff --numstat: {}", e);
        return GitDiffStatResult { is_repo: true, ..Default::default() };
    }
    let mut additions = 0u64;
    let mut deletions = 0u64;
    for line in r.stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut it = trimmed.splitn(3, '\t');
        let a = it.next().unwrap_or("");
        let d = it.next().unwrap_or("");
        if a == "-" || d == "-" {
            continue; /* binary */
        }
        additions += a.parse::<u64>().unwrap_or(0);
        deletions += d.parse::<u64>().unwrap_or(0);
    }
    GitDiffStatResult { is_repo: true, additions, deletions }
}

/* ---------------- push ---------------- */

fn has_upstream(ws_path: &str) -> bool {
    match run_process(ws_path, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"], None) {
        Ok(r) => r.code == 0 && !r.stdout.trim().is_empty(),
        Err(_) => false,
    }
}

pub fn push(ws_path: &str, set_upstream: bool) -> GitCommandResult {
    if git_dir_for(ws_path).is_none() {
        return GitCommandResult { ok: false, stderr: "Not a git repository".into(), ..Default::default() };
    }
    let mut args: Vec<String> = vec!["push".to_string()];
    if set_upstream && !has_upstream(ws_path) {
        if let Some(branch) = status(ws_path).branch {
            args.push("--set-upstream".to_string());
            args.push("origin".to_string());
            args.push(branch);
        }
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let r = run_process(ws_path, &arg_refs, Some(120_000)).unwrap_or_default();
    GitCommandResult { ok: r.code == 0, stdout: r.stdout, stderr: r.stderr, code: r.code }
}

/* ---------------- remote info ---------------- */

pub fn remote_info(ws_path: &str) -> GitRemoteInfo {
    if git_dir_for(ws_path).is_none() {
        return GitRemoteInfo::default();
    }
    let Ok(r) = run_process(ws_path, &["remote"], None) else {
        return GitRemoteInfo { is_repo: true, ..Default::default() };
    };
    if let Err(e) = ensure_ok(&r, "git remote") {
        eprintln!("[bentomux] git remote: {}", e);
        return GitRemoteInfo { is_repo: true, ..Default::default() };
    }
    let remote = r.stdout.lines().map(|s| s.trim()).find(|s| !s.is_empty()).map(String::from);
    let Some(remote) = remote else {
        return GitRemoteInfo { is_repo: true, remote: None, default_branch: None };
    };
    let default_branch = {
        let head_arg = format!("refs/remotes/{}/HEAD", remote);
        let arg_refs: Vec<&str> = vec!["--short", &head_arg];
        run_process(ws_path, &arg_refs, None)
            .ok()
            .and_then(|h| if h.code == 0 { h.stdout.trim().split('/').last().map(String::from) } else { None })
    };
    GitRemoteInfo { is_repo: true, remote: Some(remote), default_branch }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo(tag: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "bentomux-git-{}-{}-{}",
            tag,
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.to_string_lossy().into_owned();
        run_process(&p, &["init", "-q"], None).expect("git init");
        run_process(&p, &["config", "user.email", "test@bentomux"], None).ok();
        run_process(&p, &["config", "user.name", "test"], None).ok();
        /* hermetic vs the machine's global config: commit.gpgsign=true makes
           every commit spawn the agent's pinentry, which blocks headless runs */
        run_process(&p, &["config", "commit.gpgsign", "false"], None).ok();
        run_process(&p, &["config", "tag.gpgsign", "false"], None).ok();
        p
    }

    fn write_file(dir: &str, name: &str, content: &str) {
        std::fs::write(std::path::Path::new(dir).join(name), content).unwrap();
    }

    #[test]
    fn git_dir_and_branch_detection() {
        let repo = temp_repo("branch");
        assert!(git_dir_for(&repo).is_some());
        let branch = read_branch(&repo).unwrap_or_default();
        assert!(branch == "master" || branch == "main");

        let not_repo = std::env::temp_dir().to_string_lossy().into_owned();
        assert!(git_dir_for(&not_repo).is_none());
        assert_eq!(read_branch(&not_repo), None);
    }

    #[test]
    fn git_status_reports_changes() {
        let repo = temp_repo("status");
        write_file(&repo, "a.txt", "hello\n");
        let st = status(&repo);
        assert!(st.is_repo);
        assert!(st.has_untracked);
        assert_eq!(st.entries.len(), 1);
        assert_eq!(st.entries[0].status, "??");
        assert_eq!(st.entries[0].path, "a.txt");
        assert!(st.branch.is_some());
    }

    #[test]
    fn git_diff_and_stat() {
        let repo = temp_repo("diff");
        write_file(&repo, "a.txt", "one\n");
        run_process(&repo, &["add", "a.txt"], None).unwrap();
        run_process(&repo, &["commit", "-qm", "init"], None).unwrap();
        write_file(&repo, "a.txt", "one\ntwo\n");

        let files = diff(&repo, None);
        assert!(files.is_repo);
        assert_eq!(files.files.len(), 1);
        assert_eq!(files.files[0].status, "M");
        assert!(files.files[0].patch.contains("diff --git"));

        let ds = diff_stat(&repo);
        assert!(ds.is_repo);
        assert!(ds.additions >= 1);
    }

    #[test]
    fn git_remote_info_empty_repo() {
        let repo = temp_repo("remote");
        let info = remote_info(&repo);
        assert!(info.is_repo);
        assert_eq!(info.remote, None);
    }

    /* porcelain -z splits renames across NUL segments. Faithful to the Electron
       port: the original code consumed the next NUL segment as the path,
       which for git's `R <target>\0<source>` shape yields the source name. */
    #[test]
    fn git_status_reports_rename() {
        let repo = temp_repo("rename");
        write_file(&repo, "old.txt", "hi\n");
        run_process(&repo, &["add", "old.txt"], None).unwrap();
        run_process(&repo, &["commit", "-qm", "init"], None).unwrap();
        run_process(&repo, &["mv", "old.txt", "moved.txt"], None).unwrap();
        let st = status(&repo);
        assert!(st.is_repo);
        assert!(st.entries.iter().any(|e| e.index_status == "R"),
            "rename detected with index R: {:?}", st.entries);
    }

    #[test]
    fn parse_status_branch_line() {
        let parts = vec![
            "## main...origin/main [ahead 3, behind 1]".to_string(),
            " M file.ts".to_string(),
        ];
        let (branch, ahead, behind, entries, _) = parse_status(&parts);
        assert_eq!(branch.as_deref(), Some("main"));
        assert_eq!(ahead, 3);
        assert_eq!(behind, 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].index_status, " ");
        assert_eq!(entries[0].work_tree_status, "M");
    }

    #[test]
    fn parse_status_untracked_and_rename() {
        let parts = vec![
            "## main".to_string(),
            " x1".to_string(),
            "?? new.txt".to_string(),
            "R  old.txt".to_string(),
            "renamed.txt".to_string(),
        ];
        let (_, _, _, entries, has_untracked) = parse_status(&parts);
        assert!(has_untracked);
        assert!(entries.iter().any(|e| e.path == "new.txt"));
        assert!(entries.iter().any(|e| e.path == "renamed.txt"));
    }

    /* the fs.watch on .git/HEAD fires an instant branch event, not just the 4s poll */
    #[test]
    fn branch_watcher_fires_on_checkout() {
        let repo = temp_repo("watch");
        let main = read_branch(&repo).unwrap();
        assert!(main == "master" || main == "main");

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Option<String>>::new()));
        let seen2 = seen.clone();
        on_branch_change(move |_, branch| {
            seen2.lock().unwrap().push(branch.map(String::from));
        });
        watch_workspace("watch-ws", &repo);

        /* new branch updates .git/HEAD → the fs watcher should notice quickly */
        run_process(&repo, &["checkout", "-qb", "feature-x"], None).unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        loop {
            if seen.lock().unwrap().iter().any(|b| b.as_deref() == Some("feature-x")) {
                unwatch_workspace("watch-ws");
                return;
            }
            if std::time::Instant::now() >= deadline {
                unwatch_workspace("watch-ws");
                panic!("branch watcher never reported the feature-x branch");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}