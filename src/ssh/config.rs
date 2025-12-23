use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use regex::Regex;

use crate::server::Server;

/// Parse the SSH config file and extract hosts
pub fn parse_ssh_config() -> Result<Vec<Server>> {
    let config_path = get_ssh_config_path()?;
    let content = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read SSH config at {:?}", config_path))?;

    parse_config_content(&content)
}

/// Get the path to the SSH config file
fn get_ssh_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".ssh").join("config"))
}

/// Parse the content of an SSH config file
fn parse_config_content(content: &str) -> Result<Vec<Server>> {
    let mut servers = Vec::new();
    let mut current_host: Option<String> = None;
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
                // Save previous host if exists
                if let Some(host) = current_host.take() {
                    // Skip wildcard hosts and patterns
                    if !host.contains('*') && !host.contains('?') {
                        let hostname = current_hostname.take().unwrap_or_else(|| host.clone());
                        let mut server = Server::new(host, hostname);
                        server.user = current_user.take();
                        server.port = current_port.take().unwrap_or(22);
                        server.identity_file = current_identity.take();
                        servers.push(server);
                    } else {
                        // Clear state for wildcard hosts
                        current_hostname = None;
                        current_user = None;
                        current_port = None;
                        current_identity = None;
                    }
                }
                current_host = Some(value);
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

    // Don't forget the last host
    if let Some(host) = current_host {
        if !host.contains('*') && !host.contains('?') {
            let hostname = current_hostname.unwrap_or_else(|| host.clone());
            let mut server = Server::new(host, hostname);
            server.user = current_user;
            server.port = current_port.unwrap_or(22);
            server.identity_file = current_identity;
            servers.push(server);
        }
    }

    Ok(servers)
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
}
