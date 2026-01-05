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

  No sudo required:
    Homebrew:  brew install mosh
    Conda:     conda install -c conda-forge mosh
    Nix:       nix-env -iA nixpkgs.mosh

  With sudo:
    Ubuntu:    sudo apt install mosh
    Fedora:    sudo dnf install mosh
    Arch:      sudo pacman -S mosh
    Alpine:    sudo apk add mosh

On remote servers, install mosh-server using the same commands.
Ensure UDP ports 60000-61000 are open for mosh connections."#
        .to_string()
}

/// Detect local package manager (prefers user-space options)
pub fn detect_local_package_manager() -> Option<&'static str> {
    // Prefer user-space package managers (no sudo needed)
    let managers = [
        ("brew", "brew"),      // macOS / Linuxbrew (no sudo)
        ("conda", "conda"),    // Conda (no sudo)
        ("mamba", "mamba"),    // Mamba (no sudo)
        ("nix-env", "nix"),    // Nix (no sudo for user profile)
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
        // User-space (no sudo)
        "brew" => "brew install mosh",
        "conda" => "conda install -y -c conda-forge mosh",
        "mamba" => "mamba install -y -c conda-forge mosh",
        "nix" => "nix-env -iA nixpkgs.mosh",
        // System (needs sudo)
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
        None => return (false, "No supported package manager found. Try: brew, conda, or nix".to_string()),
    };

    // Build command based on package manager
    let (program, args): (&str, Vec<&str>) = match pkg_manager {
        // User-space package managers (no sudo needed)
        "brew" => ("brew", vec!["install", "mosh"]),
        "conda" => ("conda", vec!["install", "-y", "-c", "conda-forge", "mosh"]),
        "mamba" => ("mamba", vec!["install", "-y", "-c", "conda-forge", "mosh"]),
        "nix" => ("nix-env", vec!["-iA", "nixpkgs.mosh"]),
        // System package managers (need sudo, use sh -c)
        _ => {
            let cmd = get_install_command(pkg_manager);
            ("sh", vec!["-c", cmd])
        }
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

    // Detect ALL available package managers
    let detect_script = r#"
echo "user:"
command -v brew >/dev/null 2>&1 && echo "brew"
command -v conda >/dev/null 2>&1 && echo "conda"
command -v mamba >/dev/null 2>&1 && echo "mamba"
command -v nix-env >/dev/null 2>&1 && echo "nix"
echo "system:"
command -v apt >/dev/null 2>&1 && echo "apt"
command -v dnf >/dev/null 2>&1 && echo "dnf"
command -v yum >/dev/null 2>&1 && echo "yum"
command -v pacman >/dev/null 2>&1 && echo "pacman"
command -v apk >/dev/null 2>&1 && echo "apk"
true
"#;

    let output = match run_remote_command(server, detect_script).await {
        Ok(o) => o,
        Err(e) => return (false, format!("Failed to detect package manager: {}", e)),
    };

    // Parse available package managers
    let mut user_space: Vec<&str> = Vec::new();
    let mut system: Vec<&str> = Vec::new();
    let mut in_user = false;
    let mut in_system = false;

    for line in output.lines() {
        let line = line.trim();
        if line == "user:" {
            in_user = true;
            in_system = false;
        } else if line == "system:" {
            in_user = false;
            in_system = true;
        } else if !line.is_empty() {
            if in_user {
                user_space.push(line);
            } else if in_system {
                system.push(line);
            }
        }
    }

    // Try user-space package managers first (no sudo needed)
    for pm in &user_space {
        let install_cmd = match *pm {
            "brew" => "brew install mosh",
            "conda" => "conda install -y -c conda-forge mosh",
            "mamba" => "mamba install -y -c conda-forge mosh",
            "nix" => "nix-env -iA nixpkgs.mosh",
            _ => continue,
        };

        match run_remote_command(server, install_cmd).await {
            Ok(_) => {
                // Verify installation
                if run_remote_command(server, "which mosh-server").await.is_ok() {
                    return (true, format!("Successfully installed mosh via {} on {}", pm, server.host));
                }
            }
            Err(_) => continue, // Try next package manager
        }
    }

    // Try system package managers with sudo
    for pm in &system {
        let cmd = match *pm {
            "apt" => "apt install -y mosh",
            "dnf" => "dnf install -y mosh",
            "yum" => "yum install -y mosh",
            "pacman" => "pacman -S --noconfirm mosh",
            "apk" => "apk add mosh",
            _ => continue,
        };

        let install_cmd = format!("sudo -n {} 2>&1", cmd);
        match run_remote_command(server, &install_cmd).await {
            Ok(output) => {
                if !output.contains("sudo:") && !output.contains("permission denied") {
                    // Verify installation
                    if run_remote_command(server, "which mosh-server").await.is_ok() {
                        return (true, format!("Successfully installed mosh via {} on {}", pm, server.host));
                    }
                }
            }
            Err(_) => continue,
        }
    }

    // All methods failed
    let suggestions = if !system.is_empty() {
        format!(
            "No install method worked. Try manually:\n  \
             • ssh {} 'sudo {} install mosh'\n  \
             • Or install conda/brew first",
            server.host,
            system[0]
        )
    } else {
        "No supported package manager found. Install brew or conda first.".to_string()
    };

    (false, suggestions)
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
