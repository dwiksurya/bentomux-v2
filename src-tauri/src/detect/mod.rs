/* ---------------- agent detection (screen manifests + headless screen) ----------------
   Rust port of src/main/detect/. rules.rs is the herdr-style rule engine,
   manifests.rs holds per-agent rule tables, screen.rs is the vt100-backed
   headless per-tab screen fed by the pty data channel. */

pub mod manifests;
pub mod rules;
pub mod screen;