mod app;
mod health;
mod history;
mod server;
mod ssh;
mod tunnel;
mod tui;

use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use arboard::Clipboard;
use crossterm::{
    event::Event,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use app::{App, SortOrder, ViewMode};
use health::{spawn_all_health_checks, spawn_health_check, HealthUpdate};
use history::History;
use server::generate_demo_servers;
use ssh::{build_groups, group_servers, launch_mosh_session, launch_ssh_session, parse_ssh_config, run_remote_command};
use tui::{draw, handle_key_event, poll_event, HandleResult};

fn print_help() {
    println!("ggoto - A blazingly fast TUI for managing SSH connections");
    println!();
    println!("USAGE:");
    println!("    ggoto [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    --demo     Run with fake demo data (for screenshots/demos)");
    println!("    --help     Print this help message");
    println!();
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let demo_mode = args.iter().any(|a| a == "--demo");

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }

    // Initialize the application
    let mut app = App::new();

    // Load connection history (skip in demo mode)
    let mut history = if demo_mode {
        History::default()
    } else {
        History::load().unwrap_or_default()
    };
    app.history = history.clone();

    // Restore sort order from history
    app.sort_order = SortOrder::from_str(history.get_sort_order());

    // Load servers
    if demo_mode {
        // Use demo servers with fake data
        let mut servers = generate_demo_servers();
        group_servers(&mut servers);
        let groups = build_groups(&servers);

        app.servers = servers;
        app.groups = groups;
        app.sort_servers();
    } else {
        // Parse SSH config
        match parse_ssh_config() {
            Ok(mut servers) => {
                if servers.is_empty() {
                    eprintln!("No SSH hosts found in ~/.ssh/config");
                    eprintln!("Add some hosts to your SSH config and try again.");
                    eprintln!();
                    eprintln!("Tip: Run with --demo to see a demo with fake servers.");
                    return Ok(());
                }

                // Group servers by name pattern
                group_servers(&mut servers);
                let groups = build_groups(&servers);

                app.servers = servers;
                app.groups = groups;
                app.sort_servers();
            }
            Err(e) => {
                eprintln!("Failed to parse SSH config: {}", e);
                eprintln!("Make sure ~/.ssh/config exists and is readable.");
                eprintln!();
                eprintln!("Tip: Run with --demo to see a demo with fake servers.");
                return Ok(());
            }
        }
    }

    // Setup terminal
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    // Create channel for health updates
    let (health_tx, mut health_rx) = mpsc::unbounded_channel::<HealthUpdate>();

    // Create channel for command output
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<Result<String>>();

    // Start initial health checks (skip in demo mode - already have fake data)
    if demo_mode {
        app.is_fetching = false;
    } else {
        app.is_fetching = true;
        spawn_all_health_checks(&app.servers, health_tx.clone());
    }

    // Track if we need to launch SSH after cleanup
    let mut ssh_target: Option<usize> = None;

    // Main event loop
    let result: Result<()> = loop {
        // Draw the UI
        terminal.draw(|frame| draw(frame, &app))?;

        // Clear expired status messages
        app.clear_expired_status();

        // Process any pending health updates (non-blocking)
        while let Ok(update) = health_rx.try_recv() {
            if update.server_idx < app.servers.len() {
                let server = &mut app.servers[update.server_idx];
                server.latency = update.latency;
                server.status = update.status;
                server.metrics = update.metrics;
                server.last_check = Some(std::time::Instant::now());
            }

            // Check if all servers have been checked
            let all_checked = app.servers.iter().all(|s| s.last_check.is_some());
            if all_checked {
                app.is_fetching = false;
            }
        }

        // Process any pending command output (non-blocking)
        while let Ok(result) = cmd_rx.try_recv() {
            app.is_running_command = false;
            match result {
                Ok(output) => {
                    app.command_output = Some(output);
                }
                Err(e) => {
                    app.command_output = Some(format!("Error: {}", e));
                }
            }
        }

        // Poll for events with short timeout
        if let Some(event) = poll_event(Duration::from_millis(100))? {
            match event {
                Event::Key(key) => {
                    let result = handle_key_event(&mut app, key);
                    match result {
                        HandleResult::Continue => {}
                        HandleResult::LaunchSsh(idx) => {
                            if demo_mode {
                                app.set_status("Demo mode: SSH connections disabled".to_string());
                            } else {
                                ssh_target = Some(idx);
                                break Ok(());
                            }
                        }
                        HandleResult::RefreshAll => {
                            if demo_mode {
                                app.set_status("Demo mode: Health checks disabled".to_string());
                            } else {
                                app.is_fetching = true;
                                // Reset check times
                                for server in &mut app.servers {
                                    server.last_check = None;
                                }
                                spawn_all_health_checks(&app.servers, health_tx.clone());
                            }
                        }
                        HandleResult::RefreshServer(idx) => {
                            if demo_mode {
                                app.set_status("Demo mode: Health checks disabled".to_string());
                            } else if idx < app.servers.len() {
                                app.servers[idx].last_check = None;
                                spawn_health_check(
                                    idx,
                                    app.servers[idx].clone(),
                                    health_tx.clone(),
                                );
                            }
                        }
                        HandleResult::ToggleFavorite => {
                            // Remember the selected server before toggling
                            let selected_host = app.selected_server().map(|s| s.host.clone());

                            app.toggle_selected_favorite();

                            // Re-sort if using Favorites sort order
                            if app.sort_order == SortOrder::Favorites {
                                app.sort_servers();

                                // Restore selection to the same server
                                if let Some(host) = selected_host {
                                    let display_order = app.display_order_servers();
                                    if let Some(pos) = display_order.iter().position(|&idx| app.servers[idx].host == host) {
                                        app.selected_index = pos;
                                    }
                                }
                            }

                            // Update history reference and save
                            history = app.history.clone();
                            if let Err(e) = history.save() {
                                app.set_status(format!("Failed to save: {}", e));
                            }
                        }
                        HandleResult::SortOrderChanged => {
                            // Save sort order to history
                            history.set_sort_order(app.sort_order.as_str());
                            if let Err(e) = history.save() {
                                app.set_status(format!("Failed to save: {}", e));
                            }
                        }
                        HandleResult::RunCommand(idx, cmd) => {
                            if demo_mode {
                                app.set_status("Demo mode: Remote commands disabled".to_string());
                            } else if idx < app.servers.len() {
                                let server = app.servers[idx].clone();
                                let tx = cmd_tx.clone();
                                app.is_running_command = true;
                                app.view_mode = ViewMode::CommandOutput;

                                // Spawn async task to run command
                                tokio::spawn(async move {
                                    let result = run_remote_command(&server, &cmd).await;
                                    let _ = tx.send(result);
                                });
                            }
                        }
                        HandleResult::CopyToClipboard => {
                            if let Some(ref output) = app.command_output {
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        if clipboard.set_text(output.clone()).is_ok() {
                                            app.set_status("Copied to clipboard".to_string());
                                        } else {
                                            app.set_status("Failed to copy".to_string());
                                        }
                                    }
                                    Err(_) => {
                                        app.set_status("Clipboard not available".to_string());
                                    }
                                }
                            }
                        }
                        HandleResult::SaveToFile(path) => {
                            if let Some(ref output) = app.command_output {
                                match fs::write(&path, output) {
                                    Ok(_) => {
                                        app.set_status(format!("Saved to {}", path));
                                    }
                                    Err(e) => {
                                        app.set_status(format!("Failed to save: {}", e));
                                    }
                                }
                            }
                        }
                        HandleResult::PipeToCommand(cmd) => {
                            if let Some(ref output) = app.command_output {
                                // Parse command and args
                                let parts: Vec<&str> = cmd.split_whitespace().collect();
                                if let Some((program, args)) = parts.split_first() {
                                    match Command::new(program)
                                        .args(args)
                                        .stdin(Stdio::piped())
                                        .stdout(Stdio::piped())
                                        .stderr(Stdio::piped())
                                        .spawn()
                                    {
                                        Ok(mut child) => {
                                            if let Some(mut stdin) = child.stdin.take() {
                                                let _ = stdin.write_all(output.as_bytes());
                                            }
                                            match child.wait_with_output() {
                                                Ok(result) => {
                                                    let stdout = String::from_utf8_lossy(&result.stdout);
                                                    let stderr = String::from_utf8_lossy(&result.stderr);
                                                    if result.status.success() {
                                                        app.command_output = Some(stdout.to_string());
                                                        app.command_server = Some(format!("local: {}", cmd));
                                                    } else {
                                                        app.command_output = Some(format!("Error:\n{}", stderr));
                                                    }
                                                }
                                                Err(e) => {
                                                    app.set_status(format!("Failed: {}", e));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            app.set_status(format!("Failed to run: {}", e));
                                        }
                                    }
                                }
                            }
                        }
                        HandleResult::OpenTunnel(idx, spec) => {
                            if demo_mode {
                                app.set_status("Demo mode: SSH tunnels disabled".to_string());
                            } else if idx < app.servers.len() {
                                let server = &app.servers[idx];
                                // Parse spec: "port", "port_start-port_end", "host:port", or "host:port_start-port_end"
                                let (remote_host, port_spec) = if spec.contains(':') {
                                    let parts: Vec<&str> = spec.splitn(2, ':').collect();
                                    (parts[0].to_string(), parts[1].to_string())
                                } else {
                                    ("localhost".to_string(), spec.clone())
                                };

                                // Parse port range (e.g., "8000-8010" or just "8000")
                                let ports: Vec<u16> = if port_spec.contains('-') {
                                    let range_parts: Vec<&str> = port_spec.splitn(2, '-').collect();
                                    let start = range_parts[0].parse::<u16>().unwrap_or(0);
                                    let end = range_parts[1].parse::<u16>().unwrap_or(0);
                                    if start > 0 && end >= start {
                                        (start..=end).collect()
                                    } else {
                                        vec![]
                                    }
                                } else {
                                    match port_spec.parse::<u16>() {
                                        Ok(p) if p > 0 => vec![p],
                                        _ => vec![],
                                    }
                                };

                                if ports.is_empty() {
                                    app.set_status("Invalid port specification".to_string());
                                } else {
                                    let mut opened = 0;
                                    let mut failed = 0;
                                    let mut last_error = String::new();

                                    // Use a group ID if opening multiple tunnels
                                    let group_id = if ports.len() > 1 {
                                        Some(app.tunnel_manager.next_group_id())
                                    } else {
                                        None
                                    };

                                    for remote_port in &ports {
                                        match app.tunnel_manager.open_tunnel(server, &remote_host, *remote_port, None, group_id) {
                                            Ok(_) => opened += 1,
                                            Err(e) => {
                                                failed += 1;
                                                last_error = e.to_string();
                                            }
                                        }
                                    }

                                    if failed == 0 {
                                        if opened == 1 {
                                            let local_port = app.tunnel_manager.tunnels.keys().max().unwrap_or(&0);
                                            app.set_status(format!(
                                                "Tunnel opened: localhost:{} → {}:{}",
                                                local_port, remote_host, ports[0]
                                            ));
                                        } else {
                                            app.set_status(format!(
                                                "Opened {} tunnels to {}:{}-{}",
                                                opened, remote_host, ports[0], ports[ports.len() - 1]
                                            ));
                                        }
                                    } else if opened > 0 {
                                        app.set_status(format!(
                                            "Opened {} tunnels, {} failed: {}",
                                            opened, failed, last_error
                                        ));
                                    } else {
                                        app.set_status(format!("Failed to open tunnels: {}", last_error));
                                    }
                                }
                            }
                        }
                        HandleResult::CloseTunnel(port) => {
                            if let Err(e) = app.tunnel_manager.close_tunnel(port) {
                                app.set_status(format!("Failed to close tunnel: {}", e));
                            } else {
                                app.set_status(format!("Closed tunnel on port {}", port));
                                // Adjust selection if needed (use display count, not raw count)
                                let display_count = app.tunnel_manager.display_count();
                                if app.selected_tunnel >= display_count && display_count > 0 {
                                    app.selected_tunnel = display_count - 1;
                                }
                            }
                        }
                        HandleResult::CloseTunnelGroup(group_id) => {
                            match app.tunnel_manager.close_group(group_id) {
                                Ok(count) => {
                                    app.set_status(format!("Closed {} tunnels in group", count));
                                    // Adjust selection if needed
                                    let display_count = app.tunnel_manager.display_count();
                                    if app.selected_tunnel >= display_count && display_count > 0 {
                                        app.selected_tunnel = display_count - 1;
                                    }
                                }
                                Err(e) => {
                                    app.set_status(format!("Failed to close group: {}", e));
                                }
                            }
                        }
                        HandleResult::CloseAllTunnels => {
                            let count = app.tunnel_manager.count();
                            if let Err(e) = app.tunnel_manager.close_all() {
                                app.set_status(format!("Failed to close tunnels: {}", e));
                            } else {
                                app.set_status(format!("Closed {} tunnels", count));
                                app.selected_tunnel = 0;
                            }
                        }
                        HandleResult::InstallMoshLocally => {
                            app.set_status("Installing mosh locally...".to_string());
                            let (success, msg) = ssh::install_mosh_locally();
                            if success {
                                app.use_mosh = true; // Enable mosh now that it's installed
                            }
                            app.set_status(msg);
                        }
                        HandleResult::InstallMoshOnServer(idx) => {
                            if demo_mode {
                                app.set_status("Demo mode: Install disabled".to_string());
                            } else if idx < app.servers.len() {
                                let server = app.servers[idx].clone();
                                let server_host = server.host.clone();
                                let tx = cmd_tx.clone();
                                app.set_status(format!("Installing mosh on {}...", server_host));

                                tokio::spawn(async move {
                                    let (success, msg) = ssh::install_mosh_remotely(&server).await;
                                    let result_msg = if success {
                                        format!("✓ {}", msg)
                                    } else {
                                        format!("✗ {}", msg)
                                    };
                                    let _ = tx.send(Ok(result_msg));
                                });

                                app.command_server = Some(format!("mosh install on {}", server_host));
                                app.is_running_command = true;
                                app.view_mode = ViewMode::CommandOutput;
                            }
                        }
                        HandleResult::InstallMoshOnAllServers => {
                            if demo_mode {
                                app.set_status("Demo mode: Install disabled".to_string());
                            } else {
                                let servers: Vec<_> = app.servers.iter()
                                    .filter(|s| s.metrics.as_ref().map(|m| m.mosh_server_path.is_none()).unwrap_or(true))
                                    .cloned()
                                    .collect();

                                if servers.is_empty() {
                                    app.set_status("All servers already have mosh installed".to_string());
                                } else {
                                    let tx = cmd_tx.clone();
                                    let count = servers.len();
                                    app.set_status(format!("Installing mosh on {} servers...", count));

                                    tokio::spawn(async move {
                                        let mut results = Vec::new();
                                        for server in servers {
                                            let (success, msg) = ssh::install_mosh_remotely(&server).await;
                                            let symbol = if success { "✓" } else { "✗" };
                                            results.push(format!("{} {}: {}", symbol, server.host, msg));
                                        }
                                        let _ = tx.send(Ok(results.join("\n")));
                                    });

                                    app.command_server = Some("mosh install on all servers".to_string());
                                    app.is_running_command = true;
                                    app.view_mode = ViewMode::CommandOutput;
                                }
                            }
                        }
                    }
                }
                Event::Resize(_, _) => {
                    // Terminal will redraw on next iteration
                }
                _ => {}
            }
        }

        if app.should_quit {
            break Ok(());
        }
    };

    // Close all tunnels before exiting
    if app.tunnel_manager.count() > 0 {
        let _ = app.tunnel_manager.close_all();
    }

    // Cleanup terminal
    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("Failed to leave alternate screen")?;
    terminal.show_cursor().context("Failed to show cursor")?;

    // Handle the result
    result?;

    // Launch SSH/Mosh if requested
    if let Some(idx) = ssh_target {
        if idx < app.servers.len() {
            let server = &app.servers[idx];

            // Record connection in history
            history.record_connection(&server.host);
            if let Err(e) = history.save() {
                eprintln!("Warning: Failed to save history: {}", e);
            }

            if app.use_mosh {
                println!("Connecting to {} via mosh...", server.host);
                if let Err(e) = launch_mosh_session(server) {
                    eprintln!("Mosh failed: {}", e);
                    eprintln!("Falling back to SSH...");
                    println!("Connecting to {}...", server.host);
                    launch_ssh_session(server)?;
                }
            } else {
                println!("Connecting to {}...", server.host);
                launch_ssh_session(server)?;
            }
        }
    }

    Ok(())
}
