use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::time::timeout;

use crate::server::Server;

/// Launch an SSH session to the given server
/// This replaces the current process with the ssh command
pub fn launch_ssh_session(server: &Server) -> Result<()> {
    let mut args = Vec::new();

    // Add user if specified
    if let Some(ref user) = server.user {
        args.push("-l".to_string());
        args.push(user.clone());
    }

    // Add port if not default
    if server.port != 22 {
        args.push("-p".to_string());
        args.push(server.port.to_string());
    }

    // Add identity file if specified
    if let Some(ref identity) = server.identity_file {
        args.push("-i".to_string());
        args.push(identity.clone());
    }

    // Add the host (use the Host alias from config, SSH will resolve it)
    args.push(server.host.clone());

    // Execute SSH
    let status = Command::new("ssh")
        .args(&args)
        .status()
        .context("Failed to execute SSH command")?;

    if !status.success() {
        anyhow::bail!("SSH exited with status: {}", status);
    }

    Ok(())
}

/// Command execution timeout in seconds
const COMMAND_TIMEOUT_SECS: u64 = 10;

/// Run a command on a remote server and return the output
pub async fn run_remote_command(server: &Server, command: &str) -> Result<String> {
    // SSH options for non-interactive use
    let mut args = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=5".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
    ];

    // Add user if specified
    if let Some(ref user) = server.user {
        args.push("-l".to_string());
        args.push(user.clone());
    }

    // Add port if not default
    if server.port != 22 {
        args.push("-p".to_string());
        args.push(server.port.to_string());
    }

    // Add identity file if specified
    if let Some(ref identity) = server.identity_file {
        args.push("-i".to_string());
        args.push(identity.clone());
    }

    // Add the host
    args.push(server.host.clone());

    // Add the command
    args.push(command.to_string());

    // Execute with timeout
    let output = timeout(
        Duration::from_secs(COMMAND_TIMEOUT_SECS),
        tokio::process::Command::new("ssh")
            .args(&args)
            .output(),
    )
    .await
    .context("Command timed out")?
    .context("Failed to execute SSH command")?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Remote command failed: {}", stderr);
    }
}
