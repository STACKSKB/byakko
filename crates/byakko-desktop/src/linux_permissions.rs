//! Request active-seat access through the OS authentication agent.
use std::{io::IsTerminal, path::Path, process::Command};

const INSTALL: &str = include_str!("../../../packaging/linux/install-seat-rule.sh");
const RULE: &str = include_str!("../../../packaging/linux/71-byakko-seat.rules");

pub(crate) fn enable() -> Result<String, String> {
    let terminal = std::io::stdin().is_terminal();
    let has_polkit = Path::new("/usr/bin/pkexec").is_file();
    let mut auth = if terminal && Path::new("/usr/bin/sudo").is_file() {
        Command::new("/usr/bin/sudo")
    } else if has_polkit {
        let mut cmd = Command::new("/usr/bin/pkexec");
        cmd.arg("--disable-internal-agent");
        cmd
    } else {
        return Err("Launch Byakko from a terminal with sudo available, or install a graphical polkit authentication agent.".into());
    };
    let status = auth
        .args([
            "/bin/sh",
            "-c",
            INSTALL,
            "byakko-keyboard-access",
            RULE.trim_end(),
        ])
        .status()
        .map_err(|e| format!("Could not start system authentication: {e}"))?;
    if status.success() {
        Ok(
            "Keyboard access rule installed. Choose Find connections or reconnect the keyboard."
                .into(),
        )
    } else if has_polkit && !terminal && status.code() == Some(126) {
        Err("Authentication cancelled.".into())
    } else {
        Err(format!(
            "System setup did not finish ({status}). Check administrator access. If no prompt appeared, launch Byakko from a terminal and try again."
        ))
    }
}
