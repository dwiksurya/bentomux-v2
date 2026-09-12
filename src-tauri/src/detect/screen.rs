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

/* viewport + lines of scrollback retained. Matches the pty's spawn size
   (pty.rs createTerm) so the headless parser stays column-aligned with
   the real terminal until the frontend's first resize (FitAddon) call
   updates both via resize_term -> detect::screen::resize. */
const ROWS: u16 = 24;
const COLS: u16 = 80;
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

/* Rendered text of the viewport (oldest first), newline-joined. */
pub fn screen_dump(id: &str) -> String {
    let guard = screens().lock().unwrap();
    match guard.get(id) {
        Some(s) => s.term.screen().contents(),
        None => String::new(),
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

/* Same viewport as screen_dump but as safe HTML: styled spans mirror the
   vt100 cell colors (xterm.js default ANSI palette) so the remote page
   looks like the desktop terminal. Escaped by construction — the only
   markup emitted are our own <span style="..."> tags; cell text is
   HTML-escaped char by char. */
pub fn screen_dump_html(id: &str) -> String {
    let guard = screens().lock().unwrap();
    let Some(s) = guard.get(id) else { return String::new() };
    let screen = s.term.screen();
    let (rows, cols) = screen.size();
    let mut out = String::new();
    let mut prev_style = String::new();
    let mut open = false;
    for r in 0..rows {
        let mut line = String::new();
        let mut line_has_content = false;
        let mut col: u16 = 0;
        while col < cols {
            let Some(cell) = screen.cell(r, col) else { break };
            if cell.is_wide_continuation() {
                col += 1;
                continue;
            }
            let takes_two = cell.is_wide();
            if cell.has_contents() || cell.fgcolor() != vt100::Color::Default
                || cell.bgcolor() != vt100::Color::Default
                || cell.bold() || cell.italic() || cell.underline() || cell.inverse() {
                line_has_content = true;
                let style = cell_style(cell);
                if style != prev_style {
                    if open { line.push_str("</span>"); open = false; }
                    if !style.is_empty() {
                        line.push_str("<span style=\"");
                        line.push_str(&style);
                        line.push_str("\">");
                        open = true;
                    }
                    prev_style = style;
                }
                line.push_str(&escape_cell(&cell.contents()));
            } else {
                if open { line.push_str("</span>"); open = false; }
                prev_style.clear();
                line.push(' ');
            }
            col += if takes_two { 2 } else { 1 };
        }
        /* strip trailing blank padding so wrapped lines don't grow gaps */
        while line.ends_with(' ') { line.pop(); }
        while line.ends_with("</span>") {
            let cut = line.len() - "</span>".len();
            let inner = &line[..cut];
            if inner.ends_with(' ') || inner.ends_with('>') {
                line.truncate(cut);
                open = false;
            } else { break; }
        }
        if open { line.push_str("</span>"); open = false; }
        prev_style.clear();
        if !line_has_content && line.trim().is_empty() {
            /* blank viewport row: keep a single empty line separator */
            if !out.is_empty() { out.push('\n'); }
        } else {
            if !out.is_empty() { out.push('\n'); }
            out.push_str(&line);
        }
    }
    while out.ends_with('\n') { out.pop(); }
    out
}

/* xterm.js default 16-color ANSI palette (its built-in Tango-like set) */
const ANSI16: [&str; 16] = [
    "#000000", "#cd0000", "#00cd00", "#cdcd00",
    "#0000ee", "#cd00cd", "#00cdcd", "#e5e5e5",
    "#7f7f7f", "#ff0000", "#00ff00", "#ffff00",
    "#5c5cff", "#ff00ff", "#00ffff", "#ffffff",
];

fn ansi_css(c: vt100::Color) -> &'static str {
    match c {
        vt100::Color::Default => "",
        vt100::Color::Idx(i) => ANSI16.get(i as usize).copied().unwrap_or(""),
        vt100::Color::Rgb(_, _, _) => "",
    }
}

fn ansi_css_owned(c: vt100::Color) -> String {
    match c {
        vt100::Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        _ => ansi_css(c).to_string(),
    }
}

fn cell_style(cell: &vt100::Cell) -> String {
    let mut s = String::new();
    let (mut fg, mut bg) = (cell.fgcolor(), cell.bgcolor());
    if cell.inverse() { std::mem::swap(&mut fg, &mut bg); }
    if fg != vt100::Color::Default {
        s.push_str("color:");
        s.push_str(&ansi_css_owned(fg));
        s.push(';');
    }
    if bg != vt100::Color::Default {
        s.push_str("background:");
        s.push_str(&ansi_css_owned(bg));
        s.push(';');
    }
    if cell.bold() { s.push_str("font-weight:bold;"); }
    if cell.italic() { s.push_str("font-style:italic;"); }
    if cell.underline() { s.push_str("text-decoration:underline;"); }
    s
}

fn escape_cell(text: &str) -> String {
    let mut e = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => e.push_str("&amp;"),
            '<' => e.push_str("&lt;"),
            '>' => e.push_str("&gt;"),
            '"' => e.push_str("&quot;"),
            _ => e.push(ch),
        }
    }
    e
}

fn has_visible_activity(chunk: &[u8]) -> bool {
    let mut i = 0;
    while i < chunk.len() {
        if chunk[i] == 0x1b {
            i += 1;
            while i < chunk.len() {
                let b = chunk[i];
                i += 1;
                if (0x40..=0x7e).contains(&b) { break; }
            }
            continue;
        }
        if chunk[i] >= 0x20 && chunk[i] != b' ' { return true; }
        i += 1;
    }
    false
}
/* Real terminal dimensions changed (xterm.js FitAddon resized the pty to
   match the actual window): resize the headless parser to match, so
   absolute-column escape codes in the following bytes land where the
   real terminal put them. FitAddon fires its first resize on mount,
   before any shell output exists, so the entry usually isn't created
   yet — create it at the requested size instead of silently dropping
   the resize, otherwise the pane is stuck at the ROWS/COLS default for
   its whole lifetime. */
pub fn resize(id: &str, cols: u16, rows: u16) {
    let mut guard = screens().lock().unwrap();
    let entry = guard.entry(id.to_string()).or_insert_with(|| ScreenState {
        term: Parser::new(rows, cols, SCROLLBACK),
        osc_title: String::new(),
        osc_progress: String::new(),
        last_data_at: 0,
        osc_carry: Vec::new(),
    });
    entry.term.set_size(rows, cols);
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
    let before_screen = entry.term.screen().contents();
    let before_title = entry.osc_title.clone();
    let before_progress = entry.osc_progress.clone();
    feed(entry, chunk);
    if has_visible_activity(chunk)
        && (before_screen != entry.term.screen().contents()
        || before_title != entry.osc_title
        || before_progress != entry.osc_progress)
    {
        entry.last_data_at = now_ms();
    }
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

    #[test]
    fn unchanged_terminal_output_does_not_refresh_activity() {
        let id = "activity-test";
        update(id, b"same");
        let first = screen_meta(id).2;
        std::thread::sleep(std::time::Duration::from_millis(2));
        update(id, b"");
        assert_eq!(screen_meta(id).2, first);
        remove(id);
    }

    #[test]
    fn resize_keeps_wide_line_unsplit_in_html_render() {
        /* screen_dump_html (the actual remote-mirror path) forces a
           row break between every viewport row regardless of vt100's
           wrap flag, so a parser stuck at the wrong (narrower) width
           visibly splits a line the real terminal drew on one row —
           this is the exact bug the user saw as broken/split output. */
        let id = "resize-html-test";
        update(id, b"seed\r\n");
        resize(id, 200, 24);
        let wide = "x".repeat(150);
        update(id, wide.as_bytes());
        let html = screen_dump_html(id);
        assert!(
            html.contains(&wide),
            "expected one unsplit 150-char run at 200 cols, got {:?}",
            html
        );
        remove(id);
    }

    #[test]
    fn resize_before_any_data_is_not_lost() {
        /* the real-world race: FitAddon fires its first resize on mount,
           before the shell has written a single byte, so no screen entry
           exists yet when resize() runs */
        let id = "resize-before-data";
        resize(id, 200, 24);
        let wide = "y".repeat(150);
        update(id, wide.as_bytes());
        let html = screen_dump_html(id);
        assert!(
            html.contains(&wide),
            "resize before first data must not be dropped, got {:?}",
            html
        );
        remove(id);
    }

    #[test]
    fn resize_alone_creates_an_empty_entry_with_no_visible_lines() {
        resize("resize-only", 200, 24);
        assert!(screen_lines("resize-only").is_empty());
        remove("resize-only");
    }
}