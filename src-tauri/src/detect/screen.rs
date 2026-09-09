/* ---------------- per-tab headless screen model ----------------
   Rust port of src/main/detect/screen.ts. Feeds raw pty output into a
   vt100 terminal parser so the main process keeps a real rendered screen
   per tab (like herdr owning the terminal buffer), extracts the OSC title
   (0/2) and ConEmu-progress (9) sequences, and exposes the bottom of the
   viewport as plain lines for manifest evaluation. `screenDump()` also
   backs the remote monitor's screen mirror.
   (ponytail: we expose the 40-row viewport, which is all the Phase 6
   manifest regions need; a larger scrollback dump for the remote monitor
   arrives in Phase 8.) */

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast;
use vt100::Parser;

/* viewport + lines of scrollback retained, mirroring xterm-headless */
const ROWS: u16 = 40;
const COLS: u16 = 120;
const SCROLLBACK: usize = 500;

struct ScreenState {
    term: Parser,
    osc_title: String,
    osc_progress: String,
    last_data_at: u64,
    /* OSC payload may span chunks; carried here until its terminator arrives */
    osc_carry: Vec<u8>,
}

fn screens() -> &'static Mutex<HashMap<String, ScreenState>> {
    static SCREENS: OnceLock<Mutex<HashMap<String, ScreenState>>> = OnceLock::new();
    SCREENS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/* Normalize one OSC payload into title/progress state. */
fn handle_osc(state: &mut ScreenState, payload: &[u8]) {
    let text = String::from_utf8_lossy(payload).into_owned();
    if let Some(rest) = text.strip_prefix("0;") {
        state.osc_title = rest.to_string();
    } else if let Some(rest) = text.strip_prefix("2;") {
        state.osc_title = rest.to_string();
    } else if let Some(rest) = text.strip_prefix("9;") {
        /* ConEmu-style progress "4;state;progress" → "state;progress";
           state 4 settles (done), 0 clears. */
        let parts: Vec<&str> = rest.split(';').collect();
        match parts.as_slice() {
            [head, rest @ ..] if *head == "4" && rest.len() >= 1 => {
                state.osc_progress = rest.join(";");
            }
            [head, ..] if *head == "0" => state.osc_progress.clear(),
            _ => {}
        }
    }
}

/* Feed a raw pty chunk: split out OSC sequences (recording title/progress),
   forward the remaining bytes to the vt100 renderer. */
fn feed(state: &mut ScreenState, chunk: &[u8]) {
    let mut stream: Vec<u8> = std::mem::take(&mut state.osc_carry);
    stream.extend_from_slice(chunk);

    let mut render = Vec::with_capacity(stream.len());
    let mut i = 0usize;
    while i < stream.len() {
        if stream[i] == 0x1b && i + 1 < stream.len() && stream[i + 1] == b']' {
            /* OSC: find its terminator — BEL (0x07) or ESC \ (0x1b 0x5c) */
            let mut end: Option<usize> = None;
            let mut j = i + 2;
            while j < stream.len() {
                if stream[j] == 0x07 {
                    end = Some(j);
                    break;
                }
                if stream[j] == 0x1b && j + 1 < stream.len() && stream[j + 1] == b'\\' {
                    end = Some(j);
                    break;
                }
                j += 1;
            }
            match end {
                Some(e) => {
                    handle_osc(state, &stream[i + 2..e]);
                    i = if stream[e] == 0x1b { e + 2 } else { e + 1 };
                }
                None => {
                    /* not yet terminated — carry the remainder across chunks */
                    state.osc_carry = stream[i..].to_vec();
                    break;
                }
            }
        } else {
            render.push(stream[i]);
            i += 1;
        }
    }
    if !render.is_empty() {
        state.term.process(&render);
    }
}

/* Non-empty visible-screen text lines, oldest first. */
pub fn screen_lines(id: &str) -> Vec<String> {
    let guard = screens().lock().unwrap();
    let Some(s) = guard.get(id) else { return Vec::new() };
    s.term.screen().contents().lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.trim().is_empty())
        .collect()
}

pub fn screen_meta(id: &str) -> (String, String, u64) {
    let guard = screens().lock().unwrap();
    match guard.get(id) {
        Some(s) => (s.osc_title.clone(), s.osc_progress.clone(), s.last_data_at),
        None => (String::new(), String::new(), 0),
    }
}

/* Rendered text of the viewport (oldest first), newline-joined. */
pub fn screen_dump(id: &str) -> String {
    let guard = screens().lock().unwrap();
    match guard.get(id) {
        Some(s) => s.term.screen().contents(),
        None => String::new(),
    }
}

fn update(id: &str, chunk: &[u8]) {
    let mut guard = screens().lock().unwrap();
    let entry = guard.entry(id.to_string()).or_insert_with(|| ScreenState {
        term: Parser::new(ROWS, COLS, SCROLLBACK),
        osc_title: String::new(),
        osc_progress: String::new(),
        last_data_at: 0,
        osc_carry: Vec::new(),
    });
    entry.last_data_at = now_ms();
    feed(entry, chunk);
}

fn remove(id: &str) {
    screens().lock().unwrap().remove(id);
}

/* ---- wiring: subscribe to the pty data/exit broadcasts and maintain the
   store. One thread per subject; each runs for the lifetime of the process.
   Broadcast receivers don't offer a blocking recv, so we poll try_recv. */
pub fn init_screen_feed(
    data_rx: broadcast::Receiver<(String, String)>,
    exit_rx: broadcast::Receiver<(String, i32)>,
) {
    std::thread::spawn(move || {
        let mut rx = data_rx;
        loop {
            match rx.try_recv() {
                Ok((id, chunk)) => update(&id, chunk.as_bytes()),
                Err(broadcast::error::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
    });
    std::thread::spawn(move || {
        let mut rx = exit_rx;
        loop {
            match rx.try_recv() {
                Ok((id, _)) => remove(&id),
                Err(broadcast::error::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_screen_returns_no_lines() {
        assert!(screen_lines("nope").is_empty());
        assert_eq!(screen_meta("nope"), (String::new(), String::new(), 0));
    }

    #[test]
    fn osc_title_and_progress_are_captured() {
        let mut s = ScreenState {
            term: Parser::new(ROWS, COLS, SCROLLBACK),
            osc_title: String::new(),
            osc_progress: String::new(),
            last_data_at: 0,
            osc_carry: Vec::new(),
        };
        handle_osc(&mut s, b"2;my title");
        assert_eq!(s.osc_title, "my title");
        handle_osc(&mut s, b"9;4;3;50");
        assert_eq!(s.osc_progress, "3;50");
        handle_osc(&mut s, b"9;0");
        assert!(s.osc_progress.is_empty());
    }

    #[test]
    fn osc_is_stripped_from_render_and_crosses_chunk_boundaries() {
        let mut s = ScreenState {
            term: Parser::new(ROWS, COLS, SCROLLBACK),
            osc_title: String::new(),
            osc_progress: String::new(),
            last_data_at: 0,
            osc_carry: Vec::new(),
        };
        /* title OSC split across two chunks; second chunk also has plain text */
        feed(&mut s, b"\x1b]0;hel");
        feed(&mut s, b"lo\x07plaintext");
        assert_eq!(s.osc_title, "hello");
        let lines = s.term.screen().contents().lines().map(|l| l.trim_end().to_string()).collect::<Vec<_>>();
        assert!(lines.iter().any(|l| l == "plaintext"));
        /* no stray OSC bytes leaked into the render */
        assert!(!lines.iter().any(|l| l.contains('\u{1b}')));
    }
}