/* ---------------- bridge address + shell env ----------------
   Rust port of src/main/bridge-config.ts. Shared by pty.rs (env
   injection) and bridge.rs (listener) without either importing the other. */

use std::collections::HashMap;

pub const BRIDGE_PANE_ENV: &str = "BENTOMUX_PANE_ID";
pub const BRIDGE_ADDR_ENV: &str = "BENTOMUX_BRIDGE";

/* dev instances with an isolated store (BENTOMUX_USER_DATA_SUFFIX / smoke)
   get their own address so hook events never cross between instances */
fn instance_suffix() -> String {
    if std::env::var_os("BENTOMUX_SMOKE").is_some() {
        "-smoke".to_string()
    } else {
        std::env::var("BENTOMUX_USER_DATA_SUFFIX").unwrap_or_default()
    }
}

pub fn bridge_address() -> String {
    if cfg!(windows) {
        format!("\\\\.\\pipe\\bentomux-bridge{}", instance_suffix())
    } else {
        std::env::temp_dir()
            .join(format!("bentomux-bridge{}.sock", instance_suffix()))
            .to_string_lossy()
            .into_owned()
    }
}

/* spread into the env of every spawned shell so hook CLIs running inside
   it know which pane they belong to and where the bridge lives */
pub fn bridge_env_for(pane_id: &str) -> HashMap<String, String> {
    HashMap::from([
        (BRIDGE_PANE_ENV.to_string(), pane_id.to_string()),
        (BRIDGE_ADDR_ENV.to_string(), bridge_address()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn test_unix_socket_address() {
        std::env::remove_var("BENTOMUX_SMOKE");
        std::env::remove_var("BENTOMUX_USER_DATA_SUFFIX");
        let addr = bridge_address();
        assert!(addr.ends_with("bentomux-bridge.sock"), "addr: {}", addr);
        assert!(addr.starts_with(std::env::temp_dir().to_string_lossy().as_ref()));
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_pipe_address() {
        std::env::remove_var("BENTOMUX_SMOKE");
        std::env::remove_var("BENTOMUX_USER_DATA_SUFFIX");
        assert_eq!(bridge_address(), "\\\\.\\pipe\\bentomux-bridge");
    }

    #[test]
    fn test_bridge_env_pairs() {
        let env = bridge_env_for("t-abc");
        assert_eq!(env.get(BRIDGE_PANE_ENV).map(String::as_str), Some("t-abc"));
        assert_eq!(env.get(BRIDGE_ADDR_ENV).map(String::as_str), Some(bridge_address().as_str()));
    }
}
