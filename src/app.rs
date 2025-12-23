use regex::Regex;

use crate::history::History;
use crate::server::{Server, ServerGroup};
use crate::tunnel::TunnelManager;

/// View mode for the TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    ServerList,
    GroupList,
    ServerDetails,
    CommandOutput,
    Tunnels,
    Help,
}

/// Sort order for server list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    Name,
    Favorites,
    RecentlyUsed,
    Latency,
    CpuUsage,
    RamUsage,
    Group,
}

impl SortOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            SortOrder::Name => "name",
            SortOrder::Favorites => "favorites",
            SortOrder::RecentlyUsed => "recent",
            SortOrder::Latency => "latency",
            SortOrder::CpuUsage => "cpu",
            SortOrder::RamUsage => "ram",
            SortOrder::Group => "group",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "name" => SortOrder::Name,
            "favorites" => SortOrder::Favorites,
            "recent" => SortOrder::RecentlyUsed,
            "latency" => SortOrder::Latency,
            "cpu" => SortOrder::CpuUsage,
            "ram" => SortOrder::RamUsage,
            "group" => SortOrder::Group,
            _ => SortOrder::Name,
        }
    }
}

/// Duration before status messages auto-clear (in seconds)
pub const STATUS_MESSAGE_TIMEOUT_SECS: u64 = 3;

/// Main application state
pub struct App {
    pub servers: Vec<Server>,
    pub groups: Vec<ServerGroup>,
    pub selected_index: usize,
    pub selected_group: usize,
    pub view_mode: ViewMode,
    pub sort_order: SortOrder,
    pub filter_text: String,
    pub is_filtering: bool,
    pub should_quit: bool,
    pub status_message: Option<String>,
    pub status_message_time: Option<std::time::Instant>,
    pub is_fetching: bool,
    pub history: History,
    // Command execution
    pub is_entering_command: bool,
    pub command_text: String,
    pub command_output: Option<String>,
    pub command_server: Option<String>,
    pub is_running_command: bool,
    // Pipe/save functionality
    pub is_entering_pipe: bool,
    pub pipe_text: String,
    pub is_saving_output: bool,
    pub save_path: String,
    // Tunnel management
    pub tunnel_manager: TunnelManager,
    pub is_entering_tunnel: bool,
    pub tunnel_input: String,  // Format: "remote_host:remote_port" or just "port"
    pub selected_tunnel: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            groups: Vec::new(),
            selected_index: 0,
            selected_group: 0,
            view_mode: ViewMode::ServerList,
            sort_order: SortOrder::Name,
            filter_text: String::new(),
            is_filtering: false,
            should_quit: false,
            status_message: None,
            status_message_time: None,
            is_fetching: false,
            history: History::default(),
            is_entering_command: false,
            command_text: String::new(),
            command_output: None,
            command_server: None,
            is_running_command: false,
            is_entering_pipe: false,
            pipe_text: String::new(),
            is_saving_output: false,
            save_path: String::new(),
            tunnel_manager: TunnelManager::new(),
            is_entering_tunnel: false,
            tunnel_input: String::new(),
            selected_tunnel: 0,
        }
    }

    /// Set a status message with auto-clear timeout
    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.status_message_time = Some(std::time::Instant::now());
    }

    /// Clear status message if it has timed out
    pub fn clear_expired_status(&mut self) {
        if let Some(time) = self.status_message_time {
            if time.elapsed().as_secs() >= STATUS_MESSAGE_TIMEOUT_SECS {
                self.status_message = None;
                self.status_message_time = None;
            }
        }
    }

    /// Start command input mode
    pub fn start_command_input(&mut self) {
        self.is_entering_command = true;
        self.command_text.clear();
    }

    /// Stop command input mode
    pub fn stop_command_input(&mut self) {
        self.is_entering_command = false;
    }

    /// Add character to command
    pub fn command_push(&mut self, c: char) {
        self.command_text.push(c);
    }

    /// Remove character from command
    pub fn command_pop(&mut self) {
        self.command_text.pop();
    }

    /// Start pipe input mode
    pub fn start_pipe_input(&mut self) {
        self.is_entering_pipe = true;
        self.pipe_text.clear();
    }

    /// Stop pipe input mode
    pub fn stop_pipe_input(&mut self) {
        self.is_entering_pipe = false;
    }

    /// Add character to pipe command
    pub fn pipe_push(&mut self, c: char) {
        self.pipe_text.push(c);
    }

    /// Remove character from pipe command
    pub fn pipe_pop(&mut self) {
        self.pipe_text.pop();
    }

    /// Start save path input mode
    pub fn start_save_input(&mut self) {
        self.is_saving_output = true;
        self.save_path.clear();
    }

    /// Stop save path input mode
    pub fn stop_save_input(&mut self) {
        self.is_saving_output = false;
    }

    /// Add character to save path
    pub fn save_path_push(&mut self, c: char) {
        self.save_path.push(c);
    }

    /// Remove character from save path
    pub fn save_path_pop(&mut self) {
        self.save_path.pop();
    }

    /// Start tunnel input mode
    pub fn start_tunnel_input(&mut self) {
        self.is_entering_tunnel = true;
        self.tunnel_input.clear();
    }

    /// Stop tunnel input mode
    pub fn stop_tunnel_input(&mut self) {
        self.is_entering_tunnel = false;
    }

    /// Add character to tunnel input
    pub fn tunnel_input_push(&mut self, c: char) {
        self.tunnel_input.push(c);
    }

    /// Remove character from tunnel input
    pub fn tunnel_input_pop(&mut self) {
        self.tunnel_input.pop();
    }

    /// Get filtered servers based on current filter text
    /// Supports regex patterns - uses simple substring match for plain text
    pub fn filtered_servers(&self) -> Vec<usize> {
        if self.filter_text.is_empty() {
            (0..self.servers.len()).collect()
        } else {
            let filter_lower = self.filter_text.to_lowercase();

            // Check if pattern contains regex metacharacters
            let has_regex_chars = self.filter_text.chars().any(|c| {
                matches!(c, '.' | '*' | '+' | '?' | '^' | '$' | '[' | ']' | '(' | ')' | '{' | '}' | '|' | '\\')
            });

            // Only use regex if pattern contains metacharacters
            let regex = if has_regex_chars {
                Regex::new(&format!("(?i){}", &self.filter_text)).ok()
            } else {
                None
            };

            self.servers
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    if let Some(ref re) = regex {
                        // Use regex matching
                        re.is_match(&s.host)
                            || re.is_match(&s.hostname)
                            || s.group.as_ref().is_some_and(|g| re.is_match(g))
                    } else {
                        // Use simple substring matching (case-insensitive)
                        s.host.to_lowercase().contains(&filter_lower)
                            || s.hostname.to_lowercase().contains(&filter_lower)
                            || s.group
                                .as_ref()
                                .is_some_and(|g| g.to_lowercase().contains(&filter_lower))
                    }
                })
                .map(|(i, _)| i)
                .collect()
        }
    }

    /// Get the currently selected server
    pub fn selected_server(&self) -> Option<&Server> {
        let filtered = self.filtered_servers();
        filtered.get(self.selected_index).map(|&i| &self.servers[i])
    }

    /// Get mutable reference to selected server
    #[allow(dead_code)]
    pub fn selected_server_mut(&mut self) -> Option<&mut Server> {
        let filtered = self.filtered_servers();
        if let Some(&idx) = filtered.get(self.selected_index) {
            Some(&mut self.servers[idx])
        } else {
            None
        }
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        let count = match self.view_mode {
            ViewMode::ServerList => self.filtered_servers().len(),
            ViewMode::GroupList => self.groups.len(),
            _ => 0,
        };
        if count > 0 {
            match self.view_mode {
                ViewMode::ServerList => {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
                ViewMode::GroupList => {
                    self.selected_group = self.selected_group.saturating_sub(1);
                }
                _ => {}
            }
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        let count = match self.view_mode {
            ViewMode::ServerList => self.filtered_servers().len(),
            ViewMode::GroupList => self.groups.len(),
            _ => 0,
        };
        if count > 0 {
            match self.view_mode {
                ViewMode::ServerList => {
                    if self.selected_index < count - 1 {
                        self.selected_index += 1;
                    }
                }
                ViewMode::GroupList => {
                    if self.selected_group < count - 1 {
                        self.selected_group += 1;
                    }
                }
                _ => {}
            }
        }
    }

    /// Update status message
    #[allow(dead_code)]
    pub fn update_status(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    /// Clear status message
    #[allow(dead_code)]
    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    /// Toggle sort order
    pub fn cycle_sort_order(&mut self) {
        self.sort_order = match self.sort_order {
            SortOrder::Name => SortOrder::Favorites,
            SortOrder::Favorites => SortOrder::RecentlyUsed,
            SortOrder::RecentlyUsed => SortOrder::Latency,
            SortOrder::Latency => SortOrder::CpuUsage,
            SortOrder::CpuUsage => SortOrder::RamUsage,
            SortOrder::RamUsage => SortOrder::Group,
            SortOrder::Group => SortOrder::Name,
        };
        self.sort_servers();
    }

    /// Sort servers based on current sort order
    pub fn sort_servers(&mut self) {
        match self.sort_order {
            SortOrder::Name => {
                self.servers.sort_by(|a, b| a.host.cmp(&b.host));
            }
            SortOrder::Favorites => {
                // Sort favorites first, then by name
                self.servers.sort_by(|a, b| {
                    let a_fav = self.history.is_favorite(&a.host);
                    let b_fav = self.history.is_favorite(&b.host);
                    match (b_fav, a_fav) {
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, true) => std::cmp::Ordering::Less,
                        _ => a.host.cmp(&b.host),
                    }
                });
            }
            SortOrder::RecentlyUsed => {
                // Sort by last connection time (most recent first)
                self.servers.sort_by(|a, b| {
                    let a_time = self.history.last_connected(&a.host);
                    let b_time = self.history.last_connected(&b.host);
                    // Reverse order: most recent first
                    match (b_time, a_time) {
                        (Some(b_t), Some(a_t)) => b_t.cmp(&a_t),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => a.host.cmp(&b.host), // Fall back to name
                    }
                });
            }
            SortOrder::Latency => {
                self.servers.sort_by(|a, b| {
                    a.latency_ms()
                        .unwrap_or(u64::MAX)
                        .cmp(&b.latency_ms().unwrap_or(u64::MAX))
                });
            }
            SortOrder::CpuUsage => {
                self.servers.sort_by(|a, b| {
                    let a_cpu = a.metrics.as_ref().map(|m| m.cpu_usage).unwrap_or(f32::MAX);
                    let b_cpu = b.metrics.as_ref().map(|m| m.cpu_usage).unwrap_or(f32::MAX);
                    a_cpu
                        .partial_cmp(&b_cpu)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortOrder::RamUsage => {
                self.servers.sort_by(|a, b| {
                    let a_ram = a
                        .metrics
                        .as_ref()
                        .map(|m| m.ram_usage_percent())
                        .unwrap_or(f32::MAX);
                    let b_ram = b
                        .metrics
                        .as_ref()
                        .map(|m| m.ram_usage_percent())
                        .unwrap_or(f32::MAX);
                    a_ram
                        .partial_cmp(&b_ram)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortOrder::Group => {
                self.servers.sort_by(|a, b| {
                    let a_group = a.group.as_deref().unwrap_or("");
                    let b_group = b.group.as_deref().unwrap_or("");
                    a_group.cmp(b_group).then_with(|| a.host.cmp(&b.host))
                });
            }
        }
    }

    /// Enter filter mode
    pub fn start_filtering(&mut self) {
        self.is_filtering = true;
        self.filter_text.clear();
    }

    /// Exit filter mode
    pub fn stop_filtering(&mut self) {
        self.is_filtering = false;
    }

    /// Add character to filter
    pub fn filter_push(&mut self, c: char) {
        self.filter_text.push(c);
        self.selected_index = 0;
    }

    /// Remove character from filter
    pub fn filter_pop(&mut self) {
        self.filter_text.pop();
        self.selected_index = 0;
    }

    /// Clear filter
    pub fn filter_clear(&mut self) {
        self.filter_text.clear();
        self.selected_index = 0;
    }

    /// Toggle favorite for the currently selected server
    pub fn toggle_selected_favorite(&mut self) {
        let filtered = self.filtered_servers();
        if let Some(&idx) = filtered.get(self.selected_index) {
            let host = self.servers[idx].host.clone();
            self.history.toggle_favorite(&host);
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_contains_substring() {
        let mut app = App::new();
        app.servers = vec![
            Server::new("ysdc1".to_string(), "ysdc1.example.com".to_string()),
            Server::new("ysdc2".to_string(), "ysdc2.example.com".to_string()),
            Server::new("prod-web".to_string(), "prod.example.com".to_string()),
        ];

        // Filter by "1" should match "ysdc1"
        app.filter_text = "1".to_string();
        let filtered = app.filtered_servers();
        assert_eq!(filtered.len(), 1);
        assert_eq!(app.servers[filtered[0]].host, "ysdc1");

        // Filter by "ysdc" should match both ysdc servers
        app.filter_text = "ysdc".to_string();
        let filtered = app.filtered_servers();
        assert_eq!(filtered.len(), 2);

        // Filter by "prod" should match prod-web
        app.filter_text = "prod".to_string();
        let filtered = app.filtered_servers();
        assert_eq!(filtered.len(), 1);
        assert_eq!(app.servers[filtered[0]].host, "prod-web");

        // Empty filter should return all
        app.filter_text = String::new();
        let filtered = app.filtered_servers();
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_filter_regex() {
        let mut app = App::new();
        app.servers = vec![
            Server::new("ysdc1".to_string(), "ysdc1.example.com".to_string()),
            Server::new("ysdc2".to_string(), "ysdc2.example.com".to_string()),
            Server::new("prod-web".to_string(), "prod.example.com".to_string()),
        ];

        // Regex: ysdc. should match both ysdc servers
        app.filter_text = "ysdc.".to_string();
        let filtered = app.filtered_servers();
        assert_eq!(filtered.len(), 2);

        // Regex: ^prod should match prod-web
        app.filter_text = "^prod".to_string();
        let filtered = app.filtered_servers();
        assert_eq!(filtered.len(), 1);
    }
}
