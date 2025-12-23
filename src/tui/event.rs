use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, ViewMode};
use crate::tunnel::TunnelDisplayItem;

/// Poll for terminal events with timeout
pub fn poll_event(timeout: Duration) -> Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

/// Handle keyboard input
pub fn handle_key_event(app: &mut App, key: KeyEvent) -> HandleResult {
    // Handle filter mode separately
    if app.is_filtering {
        return handle_filter_input(app, key);
    }

    // Handle command input mode
    if app.is_entering_command {
        return handle_command_input(app, key);
    }

    // Handle pipe input mode
    if app.is_entering_pipe {
        return handle_pipe_input(app, key);
    }

    // Handle save path input mode
    if app.is_saving_output {
        return handle_save_input(app, key);
    }

    // Handle tunnel input mode
    if app.is_entering_tunnel {
        return handle_tunnel_input(app, key);
    }

    // Global shortcuts
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            // Clear status message to show key hints
            app.status_message = None;

            if app.view_mode == ViewMode::Help
                || app.view_mode == ViewMode::ServerDetails
                || app.view_mode == ViewMode::CommandOutput
                || app.view_mode == ViewMode::Tunnels
            {
                app.view_mode = ViewMode::ServerList;
            } else {
                app.should_quit = true;
            }
            return HandleResult::Continue;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return HandleResult::Continue;
        }
        _ => {}
    }

    match app.view_mode {
        ViewMode::ServerList => handle_server_list_input(app, key),
        ViewMode::GroupList => handle_group_list_input(app, key),
        ViewMode::ServerDetails => handle_details_input(app, key),
        ViewMode::CommandOutput => handle_command_output_input(app, key),
        ViewMode::Tunnels => handle_tunnels_input(app, key),
        ViewMode::Help => handle_help_input(app, key),
    }
}

/// Result of handling an event
pub enum HandleResult {
    Continue,
    LaunchSsh(usize), // Index of server to connect to
    RefreshAll,
    RefreshServer(usize),
    ToggleFavorite,
    SortOrderChanged,
    RunCommand(usize, String), // Server index and command to run
    CopyToClipboard,
    SaveToFile(String),        // File path to save output
    PipeToCommand(String),     // Local command to pipe output to
    OpenTunnel(usize, String), // Server index, tunnel spec (host:port or just port)
    CloseTunnel(u16),          // Local port to close
    CloseTunnelGroup(u32),     // Group ID to close
    CloseAllTunnels,
}

fn handle_filter_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Esc => {
            app.stop_filtering();
            app.filter_clear();
        }
        KeyCode::Enter => {
            app.stop_filtering();
        }
        KeyCode::Backspace => {
            app.filter_pop();
        }
        KeyCode::Char(c) => {
            app.filter_push(c);
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_command_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Esc => {
            app.stop_command_input();
        }
        KeyCode::Enter => {
            if !app.command_text.is_empty() {
                let filtered = app.filtered_servers();
                if let Some(&idx) = filtered.get(app.selected_index) {
                    let cmd = app.command_text.clone();
                    app.stop_command_input();
                    app.command_server = Some(app.servers[idx].host.clone());
                    return HandleResult::RunCommand(idx, cmd);
                }
            }
            app.stop_command_input();
        }
        KeyCode::Backspace => {
            app.command_pop();
        }
        KeyCode::Char(c) => {
            app.command_push(c);
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_server_list_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.select_previous();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.select_next();
        }
        KeyCode::Enter => {
            let filtered = app.filtered_servers();
            if let Some(&idx) = filtered.get(app.selected_index) {
                return HandleResult::LaunchSsh(idx);
            }
        }
        KeyCode::Char('/') => {
            app.start_filtering();
        }
        KeyCode::Char('n') => {
            // Next search result
            app.select_next();
        }
        KeyCode::Char('N') => {
            // Previous search result
            app.select_previous();
        }
        KeyCode::Char('G') => {
            app.view_mode = ViewMode::GroupList;
            app.selected_group = 0;
        }
        KeyCode::Char('s') => {
            app.cycle_sort_order();
            return HandleResult::SortOrderChanged;
        }
        KeyCode::Char('f') => {
            return HandleResult::ToggleFavorite;
        }
        KeyCode::Char('c') => {
            // Enter command input mode
            app.start_command_input();
        }
        KeyCode::Char('t') => {
            // Enter tunnel input mode
            app.start_tunnel_input();
        }
        KeyCode::Char('T') => {
            // View all tunnels
            app.view_mode = ViewMode::Tunnels;
            app.selected_tunnel = 0;
        }
        KeyCode::Char(ch) if ch.is_ascii_lowercase() && ch != 's' && ch != 'j' && ch != 'k' && ch != 'n' && ch != 'q' && ch != 'r' && ch != 'd' && ch != 'g' && ch != 'f' && ch != 'c' && ch != 't' => {
            // Shortcut keys a-z (excluding reserved keys) to jump to server
            let idx = (ch as u8 - b'a') as usize;
            let filtered = app.filtered_servers();
            if idx < filtered.len() {
                app.selected_index = idx;
                // Immediately connect
                return HandleResult::LaunchSsh(filtered[idx]);
            }
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            // Shortcut keys 0-9 for servers 26-35
            let idx = 26 + (c as u8 - b'0') as usize;
            let filtered = app.filtered_servers();
            if idx < filtered.len() {
                app.selected_index = idx;
                return HandleResult::LaunchSsh(filtered[idx]);
            }
        }
        KeyCode::Char('r') => {
            return HandleResult::RefreshAll;
        }
        KeyCode::Char('R') => {
            let filtered = app.filtered_servers();
            if let Some(&idx) = filtered.get(app.selected_index) {
                return HandleResult::RefreshServer(idx);
            }
        }
        KeyCode::Char('d') | KeyCode::Char(' ') => {
            app.view_mode = ViewMode::ServerDetails;
        }
        KeyCode::Char('?') => {
            app.view_mode = ViewMode::Help;
        }
        KeyCode::Home => {
            app.selected_index = 0;
        }
        KeyCode::End => {
            let count = app.filtered_servers().len();
            if count > 0 {
                app.selected_index = count - 1;
            }
        }
        KeyCode::PageUp => {
            app.selected_index = app.selected_index.saturating_sub(10);
        }
        KeyCode::PageDown => {
            let count = app.filtered_servers().len();
            app.selected_index = (app.selected_index + 10).min(count.saturating_sub(1));
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_group_list_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.select_previous();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.select_next();
        }
        KeyCode::Enter | KeyCode::Char('l') => {
            // Switch to server view filtered by this group
            if let Some(group) = app.groups.get(app.selected_group) {
                app.filter_text = group.name.clone();
                app.view_mode = ViewMode::ServerList;
                app.selected_index = 0;
            }
        }
        KeyCode::Esc | KeyCode::Char('h') => {
            app.status_message = None;
            app.view_mode = ViewMode::ServerList;
        }
        KeyCode::Char('?') => {
            app.view_mode = ViewMode::Help;
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_details_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('d') | KeyCode::Char(' ') => {
            app.status_message = None;
            app.view_mode = ViewMode::ServerList;
        }
        KeyCode::Enter => {
            let filtered = app.filtered_servers();
            if let Some(&idx) = filtered.get(app.selected_index) {
                return HandleResult::LaunchSsh(idx);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.select_previous();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.select_next();
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            let filtered = app.filtered_servers();
            if let Some(&idx) = filtered.get(app.selected_index) {
                return HandleResult::RefreshServer(idx);
            }
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_help_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.status_message = None;
            app.view_mode = ViewMode::ServerList;
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_command_output_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.view_mode = ViewMode::ServerList;
            app.command_output = None;
        }
        KeyCode::Char('c') => {
            // Run another command on the same server
            app.start_command_input();
            app.view_mode = ViewMode::ServerList;
        }
        KeyCode::Char('y') => {
            // Copy output to clipboard
            return HandleResult::CopyToClipboard;
        }
        KeyCode::Char('>') => {
            // Save output to file
            app.start_save_input();
        }
        KeyCode::Char('|') => {
            // Pipe output to local command
            app.start_pipe_input();
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_pipe_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Esc => {
            app.stop_pipe_input();
        }
        KeyCode::Enter => {
            if !app.pipe_text.is_empty() {
                let cmd = app.pipe_text.clone();
                app.stop_pipe_input();
                return HandleResult::PipeToCommand(cmd);
            }
            app.stop_pipe_input();
        }
        KeyCode::Backspace => {
            app.pipe_pop();
        }
        KeyCode::Char(c) => {
            app.pipe_push(c);
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_save_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Esc => {
            app.stop_save_input();
        }
        KeyCode::Enter => {
            if !app.save_path.is_empty() {
                let path = app.save_path.clone();
                app.stop_save_input();
                return HandleResult::SaveToFile(path);
            }
            app.stop_save_input();
        }
        KeyCode::Backspace => {
            app.save_path_pop();
        }
        KeyCode::Char(c) => {
            app.save_path_push(c);
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_tunnel_input(app: &mut App, key: KeyEvent) -> HandleResult {
    match key.code {
        KeyCode::Esc => {
            app.stop_tunnel_input();
        }
        KeyCode::Enter => {
            if !app.tunnel_input.is_empty() {
                let filtered = app.filtered_servers();
                if let Some(&idx) = filtered.get(app.selected_index) {
                    let spec = app.tunnel_input.clone();
                    app.stop_tunnel_input();
                    return HandleResult::OpenTunnel(idx, spec);
                }
            }
            app.stop_tunnel_input();
        }
        KeyCode::Backspace => {
            app.tunnel_input_pop();
        }
        KeyCode::Char(c) => {
            app.tunnel_input_push(c);
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_tunnels_input(app: &mut App, key: KeyEvent) -> HandleResult {
    let display_items = app.tunnel_manager.get_display_items();
    let display_count = display_items.len();

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.view_mode = ViewMode::ServerList;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.selected_tunnel > 0 {
                app.selected_tunnel -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if display_count > 0 && app.selected_tunnel < display_count - 1 {
                app.selected_tunnel += 1;
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            // Close selected tunnel or group
            if let Some(item) = display_items.get(app.selected_tunnel) {
                match item {
                    TunnelDisplayItem::Single { local_port, .. } => {
                        return HandleResult::CloseTunnel(*local_port);
                    }
                    TunnelDisplayItem::Group { group_id, .. } => {
                        return HandleResult::CloseTunnelGroup(*group_id);
                    }
                }
            }
        }
        KeyCode::Char('D') => {
            // Close all tunnels
            return HandleResult::CloseAllTunnels;
        }
        KeyCode::Char('t') => {
            // Open new tunnel (go back to server list)
            app.view_mode = ViewMode::ServerList;
            app.start_tunnel_input();
        }
        _ => {}
    }
    HandleResult::Continue
}
