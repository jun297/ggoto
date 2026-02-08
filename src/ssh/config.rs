use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use glob::glob;
use regex::Regex;

use crate::server::Server;

const MAX_INCLUDE_DEPTH: u32 = 10;

/// Known git hosting domains to auto-filter from server list
const GIT_HOSTING_DOMAINS: &[&str] = &[
    "github.com",
    "gitlab.com",
    "bitbucket.org",
    "ssh.dev.azure.com",
    "codeberg.org",
];

/// Parse the SSH config file and extract hosts
pub fn parse_ssh_config() -> Result<Vec<Server>> {
    let config_path = get_ssh_config_path()?;
    let content = resolve_includes(&config_path, 0)?;

    let mut servers = parse_config_content(&content)?;

    // Filter out known git hosting services
    servers.retain(|s| !GIT_HOSTING_DOMAINS.contains(&s.hostname.as_str()));

    Ok(servers)
}

/// Get the path to the SSH config file
fn get_ssh_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".ssh").join("config"))
}

/// Read an SSH config file and recursively resolve Include directives
fn resolve_includes(path: &Path, depth: u32) -> Result<String> {
    if depth > MAX_INCLUDE_DEPTH {
        return Ok(String::new());
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read SSH config at {:?}", path))?;

    let ssh_dir = dirs::home_dir()
        .context("Could not determine home directory")?
        .join(".ssh");

    let mut result = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines for Include detection only
        if trimmed.is_empty() || trimmed.starts_with('#') {
            result.push_str(line);
            result.push('\n');
            continue;
        }

        let parts: Vec<&str> = trimmed.splitn(2, |c: char| c.is_whitespace() || c == '=').collect();
        if parts.len() < 2 {
            result.push_str(line);
            result.push('\n');
            continue;
        }

        let key = parts[0].to_lowercase();
        let value = parts[1].trim().trim_matches('"');

        if key == "include" {
            // Resolve the include path
            let pattern = if Path::new(value).is_absolute() {
                value.to_string()
            } else if let Some(stripped) = value.strip_prefix("~/") {
                if let Some(home) = dirs::home_dir() {
                    home.join(stripped).to_string_lossy().to_string()
                } else {
                    continue;
                }
            } else {
                ssh_dir.join(value).to_string_lossy().to_string()
            };

            // Expand glob patterns
            match glob(&pattern) {
                Ok(paths) => {
                    for entry in paths.flatten() {
                        if entry.is_file() {
                            if let Ok(included) = resolve_includes(&entry, depth + 1) {
                                result.push_str(&included);
                            }
                        }
                    }
                }
                Err(_) => {
                    // If glob fails, try as a plain path
                    let plain = PathBuf::from(&pattern);
                    if plain.is_file() {
                        if let Ok(included) = resolve_includes(&plain, depth + 1) {
                            result.push_str(&included);
                        }
                    }
                }
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    Ok(result)
}

/// Parse the content of an SSH config file
fn parse_config_content(content: &str) -> Result<Vec<Server>> {
    let mut servers = Vec::new();
    let mut current_hosts: Vec<String> = Vec::new();
    let mut current_hostname: Option<String> = None;
    let mut current_user: Option<String> = None;
    let mut current_port: Option<u16> = None;
    let mut current_identity: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse key-value pairs
        let parts: Vec<&str> = line
            .splitn(2, |c: char| c.is_whitespace() || c == '=')
            .collect();
        if parts.len() < 2 {
            continue;
        }

        let key = parts[0].to_lowercase();
        let value = parts[1].trim().trim_matches('"').to_string();

        match key.as_str() {
            "host" => {
                // Save previous hosts if any
                flush_hosts(
                    &current_hosts,
                    &mut current_hostname,
                    &mut current_user,
                    &mut current_port,
                    &mut current_identity,
                    &mut servers,
                );

                // Split multi-value Host lines (e.g., "Host git github.com")
                current_hosts = value
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
            }
            "hostname" => {
                current_hostname = Some(value);
            }
            "user" => {
                current_user = Some(value);
            }
            "port" => {
                current_port = value.parse().ok();
            }
            "identityfile" => {
                // Expand ~ to home directory
                let expanded = if let Some(stripped) = value.strip_prefix("~/") {
                    if let Some(home) = dirs::home_dir() {
                        home.join(stripped).to_string_lossy().to_string()
                    } else {
                        value
                    }
                } else {
                    value
                };
                current_identity = Some(expanded);
            }
            _ => {}
        }
    }

    // Don't forget the last host(s)
    flush_hosts(
        &current_hosts,
        &mut current_hostname,
        &mut current_user,
        &mut current_port,
        &mut current_identity,
        &mut servers,
    );

    Ok(servers)
}

/// Flush accumulated host entries into the servers list
fn flush_hosts(
    hosts: &[String],
    hostname: &mut Option<String>,
    user: &mut Option<String>,
    port: &mut Option<u16>,
    identity: &mut Option<String>,
    servers: &mut Vec<Server>,
) {
    for host in hosts {
        // Skip wildcard hosts and patterns
        if host.contains('*') || host.contains('?') {
            continue;
        }
        let hn = hostname.clone().unwrap_or_else(|| host.clone());
        let mut server = Server::new(host.clone(), hn);
        server.user = user.clone();
        server.port = port.unwrap_or(22);
        server.identity_file = identity.clone();
        servers.push(server);
    }
    *hostname = None;
    *user = None;
    *port = None;
    *identity = None;
}

/// Group servers by their name prefix
/// e.g., prod-web-01, prod-web-02 -> group "prod-web"
pub fn group_servers(servers: &mut [Server]) {
    let re = Regex::new(r"^(.+?)[-_]?\d+$").unwrap();

    for server in servers.iter_mut() {
        if let Some(caps) = re.captures(&server.host) {
            server.group = Some(caps[1].to_string());
        } else {
            // No numeric suffix - use the host itself as group
            server.group = Some(server.host.clone());
        }
    }
}

/// Build server groups from the servers list
pub fn build_groups(servers: &[Server]) -> Vec<crate::server::ServerGroup> {
    use std::collections::HashMap;

    let mut group_map: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, server) in servers.iter().enumerate() {
        if let Some(ref group_name) = server.group {
            group_map.entry(group_name.clone()).or_default().push(idx);
        }
    }

    let mut groups: Vec<crate::server::ServerGroup> = group_map
        .into_iter()
        .map(|(name, servers)| {
            let mut group = crate::server::ServerGroup::new(name);
            group.servers = servers;
            group
        })
        .collect();

    groups.sort_by(|a, b| a.name.cmp(&b.name));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_config() {
        let config = r#"
Host server1
    HostName 192.168.1.1
    User admin
    Port 2222

Host server2
    HostName example.com
    IdentityFile ~/.ssh/id_rsa
"#;
        let servers = parse_config_content(config).unwrap();
        assert_eq!(servers.len(), 2);

        assert_eq!(servers[0].host, "server1");
        assert_eq!(servers[0].hostname, "192.168.1.1");
        assert_eq!(servers[0].user, Some("admin".to_string()));
        assert_eq!(servers[0].port, 2222);

        assert_eq!(servers[1].host, "server2");
        assert_eq!(servers[1].hostname, "example.com");
        assert_eq!(servers[1].port, 22);
    }

    #[test]
    fn test_skip_wildcard() {
        let config = r#"
Host *
    ServerAliveInterval 60

Host prod-*
    User deploy

Host myserver
    HostName 10.0.0.1
"#;
        let servers = parse_config_content(config).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].host, "myserver");
    }

    #[test]
    fn test_grouping() {
        let mut servers = vec![
            Server::new("prod-web-01".to_string(), "10.0.0.1".to_string()),
            Server::new("prod-web-02".to_string(), "10.0.0.2".to_string()),
            Server::new("prod-db-01".to_string(), "10.0.1.1".to_string()),
            Server::new("standalone".to_string(), "10.0.2.1".to_string()),
        ];

        group_servers(&mut servers);

        assert_eq!(servers[0].group, Some("prod-web".to_string()));
        assert_eq!(servers[1].group, Some("prod-web".to_string()));
        assert_eq!(servers[2].group, Some("prod-db".to_string()));
        assert_eq!(servers[3].group, Some("standalone".to_string()));
    }

    #[test]
    fn test_multi_value_host() {
        let config = r#"
Host alias1 alias2
    HostName 10.0.0.1
    User admin
    Port 2222
"#;
        let servers = parse_config_content(config).unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].host, "alias1");
        assert_eq!(servers[0].hostname, "10.0.0.1");
        assert_eq!(servers[1].host, "alias2");
        assert_eq!(servers[1].hostname, "10.0.0.1");
    }

    #[test]
    fn test_filter_git_hosting() {
        let config = r#"
Host git github.com
    User git
    HostName github.com
    IdentityFile ~/.ssh/id_ed25519

Host github-personal
    HostName github.com
    User git

Host myserver
    HostName 10.0.0.1
    User admin
"#;
        let mut servers = parse_config_content(config).unwrap();
        servers.retain(|s| !GIT_HOSTING_DOMAINS.contains(&s.hostname.as_str()));

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].host, "myserver");
    }

    #[test]
    fn test_include_resolution() {
        use std::fs;

        // Create temp directory structure
        let tmp_dir = std::env::temp_dir().join("ggoto_test_include");
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir).unwrap();

        let main_config = tmp_dir.join("config");
        let included = tmp_dir.join("extra.conf");

        fs::write(
            &included,
            "Host included-server\n    HostName 10.0.0.99\n    User tester\n",
        )
        .unwrap();

        // Use absolute path in Include
        let main_content = format!(
            "Include {}\n\nHost main-server\n    HostName 10.0.0.1\n",
            included.to_string_lossy()
        );
        fs::write(&main_config, &main_content).unwrap();

        let content = resolve_includes(&main_config, 0).unwrap();
        let servers = parse_config_content(&content).unwrap();

        assert_eq!(servers.len(), 2);

        let hosts: Vec<&str> = servers.iter().map(|s| s.host.as_str()).collect();
        assert!(hosts.contains(&"included-server"));
        assert!(hosts.contains(&"main-server"));

        // Cleanup
        let _ = fs::remove_dir_all(&tmp_dir);
    }
}
