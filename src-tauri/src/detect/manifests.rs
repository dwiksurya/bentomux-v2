/* ---------------- per-agent screen manifests ----------------
   Rule strings for Claude Code are ported from herdr's bundled
   manifest (github.com/herdrdev/herdr, src/detect/manifests/claude.toml).
   Agents without a verified manifest (pi, codex, …) are identified at the
   process level and fall back to output-activity state in runtime.rs. */

use super::rules::{Matcher, RegionName, Rule, RunState, S};

fn rule(
    id: &str,
    state: RunState,
    priority: i32,
    region: RegionName,
    skip: bool,
    match_: Matcher,
) -> Rule {
    Rule { id: id.to_string(), state, priority, region, skip_state_update: skip, match_ }
}

fn contains_all(items: &[&str]) -> Matcher {
    Matcher { contains: Some(items.iter().map(|s| s.to_string()).collect()), ..Default::default() }
}

fn any_of(items: Vec<Matcher>) -> Matcher {
    Matcher { any: Some(items), ..Default::default() }
}

pub fn claude_manifest() -> super::rules::AgentManifest {
    super::rules::AgentManifest {
        agent: "claude".to_string(),
        rules: vec![
            /* transient overlay — never flips state */
            rule(
                "transcript_viewer",
                RunState::Unknown,
                1000,
                RegionName::Bottom(3),
                true,
                Matcher {
                    contains: Some(S("showing detailed transcript")),
                    any: Some(vec![
                        contains_all(&["ctrl+o", "to toggle"]),
                        contains_all(&["ctrl+e", "show all"]),
                        contains_all(&["↑↓ scroll"]),
                    ]),
                    ..Default::default()
                },
            ),
            /* busy spinner in the window title (braille ≤2.1.227, half-circles after) */
            rule(
                "osc_title_working",
                RunState::Working,
                1100,
                RegionName::OscTitle,
                false,
                Matcher { regex: Some(S("^[\\u{2800}-\\u{28FF}\\u{25D0}-\\u{25D3}] ")), ..Default::default() },
            ),
            rule(
                "live_turn_working",
                RunState::Working,
                970,
                RegionName::Bottom(12),
                false,
                Matcher {
                    any: Some(vec![
                        Matcher { line_regex: Some(S("^\\s*[⏸⏵].*esc to interrupt(?:\\s|·|$)")), ..Default::default() },
                        Matcher { line_regex: Some(S("^\\s*[\\*·✳✻✦]\\s+\\S.*…(?:\\s+\\(\\d+[smh](?:\\s|·)|\\s*$)")), ..Default::default() },
                    ]),
                    ..Default::default()
                },
            ),
            /* permission / approval prompts */
            rule(
                "permission_proceed",
                RunState::Blocked,
                980,
                RegionName::WholeRecent,
                false,
                Matcher {
                    all: Some(vec![
                        contains_all(&["do you want to proceed?"]),
                        any_of(vec![
                            Matcher { line_regex: Some(S("^\\s*(?:❯\\s*)?1\\.\\s*yes\\b")), ..Default::default() },
                            Matcher { line_regex: Some(S("^\\s*2\\.\\s*no\\b")), ..Default::default() },
                            contains_all(&["esc to cancel"]),
                        ]),
                    ]),
                    ..Default::default()
                },
            ),
            rule(
                "selection_form",
                RunState::Blocked,
                980,
                RegionName::WholeRecent,
                false,
                Matcher {
                    contains: Some(S("enter to select")),
                    any: Some(vec![
                        contains_all(&["tab/arrow keys to navigate"]),
                        contains_all(&["arrow keys to navigate"]),
                        contains_all(&["arrows to navigate"]),
                        contains_all(&["↑/↓ to navigate"]),
                        contains_all(&["↑↓ to navigate"]),
                    ]),
                    ..Default::default()
                },
            ),
            rule(
                "plan_confirm",
                RunState::Blocked,
                970,
                RegionName::WholeRecent,
                false,
                Matcher {
                    contains: Some(S("would you like to proceed?")),
                    not: Some(vec![Matcher { line_regex: Some(S("^\\s*❯\\s*$")), ..Default::default() }]),
                    ..Default::default()
                },
            ),
            /* idle prompt box inside the bottom frame */
            rule(
                "live_prompt_box",
                RunState::Idle,
                900,
                RegionName::PromptBoxBody,
                false,
                Matcher {
                    line_regex: Some(vec!["^\\s*❯".to_string(), "^\\s*[│║]\\s*[❯>]".to_string()]),
                    not: Some(vec![
                        contains_all(&["enter to select"]),
                        contains_all(&["esc to cancel"]),
                        contains_all(&["tab/arrow keys"]),
                    ]),
                    ..Default::default()
                },
            ),
            /* OSC fallbacks: ✳ title = idle; progress settled/reset (state 4 or 0) = idle */
            rule(
                "osc_title_idle",
                RunState::Idle,
                250,
                RegionName::OscTitle,
                false,
                Matcher { regex: Some(S("^[\u{2733}] ")), ..Default::default() },
            ),
            rule(
                "osc_progress_idle",
                RunState::Idle,
                250,
                RegionName::OscProgress,
                false,
                Matcher { regex: Some(vec!["^0;".to_string(), "^4;0$".to_string()]), ..Default::default() },
            ),
            /* weakest blocker evidence — anything asking a question */
            rule(
                "legacy_no_prompt_blocker",
                RunState::Blocked,
                300,
                RegionName::WholeRecent,
                false,
                Matcher {
                    any: Some(vec![
                        Matcher {
                            all: Some(vec![
                                contains_all(&["do you want to"]),
                                any_of(vec![contains_all(&["yes"]), contains_all(&["❯"])]),
                            ]),
                            ..Default::default()
                        },
                        Matcher {
                            all: Some(vec![
                                contains_all(&["would you like to"]),
                                any_of(vec![contains_all(&["yes"]), contains_all(&["❯"])]),
                            ]),
                            ..Default::default()
                        },
                        contains_all(&["waiting for permission"]),
                        contains_all(&["review your answers"]),
                    ]),
                    not: Some(vec![Matcher { line_regex: Some(S("^\\s*❯\\s*$")), ..Default::default() }]),
                    ..Default::default()
                },
            ),
        ],
    }
}

pub fn manifest_for(agent: &str) -> Option<super::rules::AgentManifest> {
    match agent {
        "claude" => Some(claude_manifest()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::rules::{evaluate, ScreenInput};

    #[test]
    fn manifest_evaluates_title_spinner_as_working() {
        let m = claude_manifest();
        let d = evaluate(&m, &ScreenInput {
            osc_title: "\u{2800} generating…".to_string(),
            osc_progress: String::new(),
            lines: vec!["some output".to_string()],
        });
        assert_eq!(d.state, Some(RunState::Working));
    }

    #[test]
    fn manifest_evaluates_permission_as_blocked() {
        let m = claude_manifest();
        let d = evaluate(&m, &ScreenInput {
            osc_title: String::new(),
            osc_progress: String::new(),
            lines: vec![
                "Do you want to proceed?".to_string(),
                "❯ 1. yes".to_string(),
                "  2. no".to_string(),
                "esc to cancel".to_string(),
            ],
        });
        assert_eq!(d.state, Some(RunState::Blocked));
    }

    #[test]
    fn manifest_idles_on_empty_prompt() {
        let m = claude_manifest();
        let d = evaluate(&m, &ScreenInput {
            osc_title: String::new(),
            osc_progress: String::new(),
            lines: vec!["plain output".to_string(), "━━━━━━━━━━━━━━".to_string(), "❯".to_string()],
        });
        assert_eq!(d.state, Some(RunState::Idle));
    }
}