use std::collections::HashMap;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result};

use crate::server::Server;

/// Default port range for tunnels
pub const DEFAULT_PORT_START: u16 = 8000;
pub const DEFAULT_PORT_END: u16 = 8100;

/// A display item for the tunnel list (either a single tunnel or a grouped range)
#[derive(Debug, Clone)]
pub enum TunnelDisplayItem {
    /// A single tunnel (no group)
    Single {
        local_port: u16,
        remote_host: String,
        remote_port: u16,
        server_host: String,
    },
    /// A group of tunnels (opened as a range)
    Group {
        group_id: u32,
        local_port_start: u16,
        local_port_end: u16,
        remote_host: String,
        remote_port_start: u16,
        remote_port_end: u16,
        server_host: String,
        count: usize,
    },
}

/// Represents an active SSH tunnel
#[derive(Debug)]
pub struct Tunnel {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub server_host: String,
    pub process: Child,
    /// Group ID for tunnels opened as a range (None = individual tunnel)
    pub group_id: Option<u32>,
}

impl Tunnel {
    /// Check if tunnel is still running
    #[allow(dead_code)]
    pub fn is_alive(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(Some(_)) => false, // Process exited
            Ok(None) => true,     // Still running
            Err(_) => false,      // Error checking
        }
    }

    /// Close the tunnel
    pub fn close(&mut self) -> Result<()> {
        self.process.kill().context("Failed to kill tunnel process")?;
        self.process.wait().context("Failed to wait for tunnel process")?;
        Ok(())
    }
}

/// Manages SSH tunnels
#[derive(Debug, Default)]
pub struct TunnelManager {
    /// Map from local port to tunnel
    pub tunnels: HashMap<u16, Tunnel>,
    /// Port range for auto-allocation
    pub port_start: u16,
    pub port_end: u16,
    /// Counter for group IDs
    next_group_id: u32,
}

impl TunnelManager {
    pub fn new() -> Self {
        Self {
            tunnels: HashMap::new(),
            port_start: DEFAULT_PORT_START,
            port_end: DEFAULT_PORT_END,
            next_group_id: 1,
        }
    }

    /// Get the next group ID for a batch of tunnels
    pub fn next_group_id(&mut self) -> u32 {
        let id = self.next_group_id;
        self.next_group_id += 1;
        id
    }

    /// Find an available port in the range
    pub fn find_available_port(&self) -> Option<u16> {
        for port in self.port_start..=self.port_end {
            // Skip if already used by a tunnel
            if self.tunnels.contains_key(&port) {
                continue;
            }
            // Check if port is actually available
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                return Some(port);
            }
        }
        None
    }

    /// Open a new tunnel
    pub fn open_tunnel(
        &mut self,
        server: &Server,
        remote_host: &str,
        remote_port: u16,
        local_port: Option<u16>,
        group_id: Option<u32>,
    ) -> Result<u16> {
        let local_port = match local_port {
            Some(p) => p,
            None => self
                .find_available_port()
                .context("No available ports in range")?,
        };

        // Build SSH tunnel command
        let mut args = vec![
            "-N".to_string(),        // No remote command
            "-L".to_string(),        // Local port forwarding
            format!("{}:{}:{}", local_port, remote_host, remote_port),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-o".to_string(),
            "ExitOnForwardFailure=yes".to_string(),
            "-o".to_string(),
            "ServerAliveInterval=30".to_string(),
            "-o".to_string(),
            "ServerAliveCountMax=3".to_string(),
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

        let process = Command::new("ssh")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start SSH tunnel")?;

        let tunnel = Tunnel {
            local_port,
            remote_host: remote_host.to_string(),
            remote_port,
            server_host: server.host.clone(),
            process,
            group_id,
        };

        self.tunnels.insert(local_port, tunnel);
        Ok(local_port)
    }

    /// Close a tunnel by local port
    pub fn close_tunnel(&mut self, local_port: u16) -> Result<()> {
        if let Some(mut tunnel) = self.tunnels.remove(&local_port) {
            tunnel.close()?;
        }
        Ok(())
    }

    /// Close all tunnels in a group
    pub fn close_group(&mut self, group_id: u32) -> Result<usize> {
        let ports: Vec<u16> = self
            .tunnels
            .iter()
            .filter(|(_, t)| t.group_id == Some(group_id))
            .map(|(&p, _)| p)
            .collect();

        let count = ports.len();
        for port in ports {
            self.close_tunnel(port)?;
        }
        Ok(count)
    }

    /// Close all tunnels for a specific server
    #[allow(dead_code)]
    pub fn close_server_tunnels(&mut self, server_host: &str) -> Result<()> {
        let ports: Vec<u16> = self
            .tunnels
            .iter()
            .filter(|(_, t)| t.server_host == server_host)
            .map(|(&p, _)| p)
            .collect();

        for port in ports {
            self.close_tunnel(port)?;
        }
        Ok(())
    }

    /// Close all tunnels
    pub fn close_all(&mut self) -> Result<()> {
        let ports: Vec<u16> = self.tunnels.keys().copied().collect();
        for port in ports {
            self.close_tunnel(port)?;
        }
        Ok(())
    }

    /// Get tunnels for a specific server
    #[allow(dead_code)]
    pub fn get_server_tunnels(&self, server_host: &str) -> Vec<&Tunnel> {
        self.tunnels
            .values()
            .filter(|t| t.server_host == server_host)
            .collect()
    }

    /// Clean up dead tunnels
    #[allow(dead_code)]
    pub fn cleanup_dead(&mut self) {
        self.tunnels.retain(|_, tunnel| tunnel.is_alive());
    }

    /// Get total tunnel count
    pub fn count(&self) -> usize {
        self.tunnels.len()
    }

    /// Get display items (grouped tunnels collapsed into single items)
    pub fn get_display_items(&self) -> Vec<TunnelDisplayItem> {
        use std::collections::BTreeMap;

        let mut items: Vec<TunnelDisplayItem> = Vec::new();
        let mut groups: BTreeMap<u32, Vec<&Tunnel>> = BTreeMap::new();

        // Sort tunnels by local port
        let mut sorted_tunnels: Vec<_> = self.tunnels.values().collect();
        sorted_tunnels.sort_by_key(|t| t.local_port);

        // Separate grouped and ungrouped tunnels
        for tunnel in sorted_tunnels {
            if let Some(group_id) = tunnel.group_id {
                groups.entry(group_id).or_default().push(tunnel);
            } else {
                items.push(TunnelDisplayItem::Single {
                    local_port: tunnel.local_port,
                    remote_host: tunnel.remote_host.clone(),
                    remote_port: tunnel.remote_port,
                    server_host: tunnel.server_host.clone(),
                });
            }
        }

        // Add grouped tunnels
        for (group_id, tunnels) in groups {
            if tunnels.is_empty() {
                continue;
            }

            let first = tunnels.first().unwrap();
            let last = tunnels.last().unwrap();

            items.push(TunnelDisplayItem::Group {
                group_id,
                local_port_start: first.local_port,
                local_port_end: last.local_port,
                remote_host: first.remote_host.clone(),
                remote_port_start: first.remote_port,
                remote_port_end: last.remote_port,
                server_host: first.server_host.clone(),
                count: tunnels.len(),
            });
        }

        // Sort by first port
        items.sort_by_key(|item| match item {
            TunnelDisplayItem::Single { local_port, .. } => *local_port,
            TunnelDisplayItem::Group { local_port_start, .. } => *local_port_start,
        });

        items
    }

    /// Get number of display items (groups count as 1)
    pub fn display_count(&self) -> usize {
        self.get_display_items().len()
    }
}
