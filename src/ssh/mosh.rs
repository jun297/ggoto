use std::process::Command;

use anyhow::{Context, Result};

use crate::server::Server;

/// Check if mosh is installed locally
pub fn is_mosh_installed() -> bool {
    Command::new("which")
        .arg("mosh")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if mosh-server is available on a remote server
/// This runs `which mosh-server` via SSH
#[allow(dead_code)]
pub async fn check_server_mosh(server: &Server) -> bool {
    use crate::ssh::run_remote_command;
    run_remote_command(server, "which mosh-server")
        .await
        .is_ok()
}

/// Get install instructions for common package managers
pub fn get_install_instructions() -> String {
    r#"Install mosh on your system:

  macOS:     brew install mosh
  Ubuntu:    sudo apt install mosh
  Fedora:    sudo dnf install mosh
  Arch:      sudo pacman -S mosh
  Alpine:    sudo apk add mosh

On remote servers, install mosh-server using the same commands.
Ensure UDP ports 60000-61000 are open for mosh connections."#
        .to_string()
}

/// Detect local package manager
pub fn detect_local_package_manager() -> Option<&'static str> {
    let managers = [
        ("brew", "brew"),      // macOS / Linuxbrew
        ("apt", "apt"),        // Debian/Ubuntu
        ("dnf", "dnf"),        // Fedora
        ("yum", "yum"),        // RHEL/CentOS
        ("pacman", "pacman"),  // Arch
        ("apk", "apk"),        // Alpine
    ];

    for (cmd, name) in managers {
        if Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(name);
        }
    }
    None
}

/// Get the install command for a package manager
pub fn get_install_command(pkg_manager: &str) -> &'static str {
    match pkg_manager {
        "brew" => "brew install mosh",
        "apt" => "sudo apt install -y mosh",
        "dnf" => "sudo dnf install -y mosh",
        "yum" => "sudo yum install -y mosh",
        "pacman" => "sudo pacman -S --noconfirm mosh",
        "apk" => "sudo apk add mosh",
        _ => "echo 'Unknown package manager'",
    }
}

/// Install mosh locally
/// Returns (success, output_message)
pub fn install_mosh_locally() -> (bool, String) {
    let pkg_manager = match detect_local_package_manager() {
        Some(pm) => pm,
        None => return (false, "No supported package manager found".to_string()),
    };

    let cmd = get_install_command(pkg_manager);

    // For brew, we don't need sudo
    let (program, args): (&str, Vec<&str>) = if pkg_manager == "brew" {
        ("brew", vec!["install", "mosh"])
    } else {
        // For other package managers, we need to use sh -c to handle sudo
        ("sh", vec!["-c", cmd])
    };

    match Command::new(program).args(&args).output() {
        Ok(output) => {
            if output.status.success() {
                (true, format!("Successfully installed mosh via {}", pkg_manager))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                (false, format!("Install failed: {}", stderr.trim()))
            }
        }
        Err(e) => (false, format!("Failed to run installer: {}", e)),
    }
}

/// Install mosh on a remote server
/// Returns (success, output_message)
pub async fn install_mosh_remotely(server: &Server) -> (bool, String) {
    use crate::ssh::run_remote_command;

    // First detect the remote package manager
    let detect_script = r#"
if command -v apt >/dev/null 2>&1; then echo "apt"
elif command -v dnf >/dev/null 2>&1; then echo "dnf"
elif command -v yum >/dev/null 2>&1; then echo "yum"
elif command -v pacman >/dev/null 2>&1; then echo "pacman"
elif command -v apk >/dev/null 2>&1; then echo "apk"
elif command -v brew >/dev/null 2>&1; then echo "brew"
else echo "unknown"
fi
"#;

    let pkg_manager = match run_remote_command(server, detect_script).await {
        Ok(output) => output.trim().to_string(),
        Err(e) => return (false, format!("Failed to detect package manager: {}", e)),
    };

    if pkg_manager == "unknown" {
        return (false, "No supported package manager found on remote server".to_string());
    }

    // Build install command - try without sudo first for brew, with sudo for others
    let install_cmd = if pkg_manager == "brew" {
        "brew install mosh".to_string()
    } else {
        // Try to install, will fail if no sudo access
        format!(
            "sudo -n {} 2>/dev/null || echo 'SUDO_REQUIRED'",
            match pkg_manager.as_str() {
                "apt" => "apt install -y mosh",
                "dnf" => "dnf install -y mosh",
                "yum" => "yum install -y mosh",
                "pacman" => "pacman -S --noconfirm mosh",
                "apk" => "apk add mosh",
                _ => return (false, "Unknown package manager".to_string()),
            }
        )
    };

    match run_remote_command(server, &install_cmd).await {
        Ok(output) => {
            if output.contains("SUDO_REQUIRED") {
                (false, format!(
                    "Sudo access required. Run manually:\n  ssh {} 'sudo {} install mosh'",
                    server.host,
                    if pkg_manager == "apt" { "apt" } else { &pkg_manager }
                ))
            } else {
                // Verify installation
                match run_remote_command(server, "which mosh-server").await {
                    Ok(_) => (true, format!("Successfully installed mosh on {}", server.host)),
                    Err(_) => (false, "Install command ran but mosh-server not found".to_string()),
                }
            }
        }
        Err(e) => (false, format!("Install failed: {}", e)),
    }
}

/// Launch a mosh session to the given server
/// This replaces the current process with the mosh command
pub fn launch_mosh_session(server: &Server) -> Result<()> {
    let mut args = Vec::new();

    // Build SSH options string for non-default settings
    let mut ssh_opts = Vec::new();

    if server.port != 22 {
        ssh_opts.push(format!("-p {}", server.port));
    }

    if let Some(ref identity) = server.identity_file {
        ssh_opts.push(format!("-i {}", identity));
    }

    // Add --ssh option if we have custom SSH settings
    if !ssh_opts.is_empty() {
        args.push("--ssh".to_string());
        args.push(format!("ssh {}", ssh_opts.join(" ")));
    }

    // Build user@host or just host
    let target = if let Some(ref user) = server.user {
        format!("{}@{}", user, server.host)
    } else {
        server.host.clone()
    };
    args.push(target);

    // Execute mosh
    let status = Command::new("mosh")
        .args(&args)
        .status()
        .context("Failed to execute mosh command")?;

    if !status.success() {
        anyhow::bail!("Mosh exited with status: {}", status);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_mosh_installed_runs() {
        // Just verify the function doesn't panic
        let _ = is_mosh_installed();
    }

    #[test]
    fn test_get_install_instructions() {
        let instructions = get_install_instructions();
        assert!(instructions.contains("brew install mosh"));
        assert!(instructions.contains("apt install mosh"));
        assert!(instructions.contains("60000-61000"));
    }
}
