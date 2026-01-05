use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, SortOrder, ViewMode};
use crate::health::format_bytes;
use crate::server::HealthStatus;
use crate::tunnel::TunnelDisplayItem;

const MAX_WIDTH: u16 = 120;

/// Constrain content to max width, aligned left
fn constrained_rect(area: Rect, max_width: u16) -> Rect {
    Rect {
        x: area.x,
        y: area.y,
        width: area.width.min(max_width),
        height: area.height,
    }
}

/// Main draw function
pub fn draw(frame: &mut Frame, app: &App) {
    let area = constrained_rect(frame.area(), MAX_WIDTH);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Status bar
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);

    match app.view_mode {
        ViewMode::ServerList => draw_server_list(frame, app, chunks[1]),
        ViewMode::GroupList => draw_group_list(frame, app, chunks[1]),
        ViewMode::ServerDetails => draw_server_details(frame, app, chunks[1]),
        ViewMode::CommandOutput => draw_command_output(frame, app, chunks[1]),
        ViewMode::Tunnels => draw_tunnels(frame, app, chunks[1]),
        ViewMode::Help => draw_help(frame, chunks[1]),
    }

    draw_status_bar(frame, app, chunks[2]);

    // Draw filter input overlay if active
    if app.is_filtering {
        draw_filter_input(frame, app);
    }

    // Draw command input overlay if active
    if app.is_entering_command {
        draw_command_input(frame, app);
    }

    // Draw pipe input overlay if active
    if app.is_entering_pipe {
        draw_pipe_input(frame, app);
    }

    // Draw save path input overlay if active
    if app.is_saving_output {
        draw_save_input(frame, app);
    }

    // Draw tunnel input overlay if active
    if app.is_entering_tunnel {
        draw_tunnel_input(frame, app);
    }

    // Draw install menu overlay if active
    if app.is_showing_install_menu {
        draw_install_menu(frame, app);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let title = match app.view_mode {
        ViewMode::ServerList => {
            let count = app.filtered_servers().len();
            let total = app.servers.len();
            if count == total {
                format!(" ggoto - {} servers ", total)
            } else {
                format!(" ggoto - {} / {} servers ", count, total)
            }
        }
        ViewMode::GroupList => format!(" ggoto - {} groups ", app.groups.len()),
        ViewMode::ServerDetails => " Server Details ".to_string(),
        ViewMode::CommandOutput => " Command Output ".to_string(),
        ViewMode::Tunnels => format!(" Tunnels ({}) ", app.tunnel_manager.count()),
        ViewMode::Help => " Help ".to_string(),
    };

    let sort_indicator = match app.sort_order {
        SortOrder::Name => "[Name]",
        SortOrder::Favorites => "[Favorites]",
        SortOrder::RecentlyUsed => "[Recent]",
        SortOrder::Latency => "[Latency]",
        SortOrder::CpuUsage => "[CPU]",
        SortOrder::RamUsage => "[RAM]",
        SortOrder::Group => "[Group]",
    };

    let header_text = format!("{} sorted by {}", title, sort_indicator);

    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan))
        .title(header_text);

    frame.render_widget(block, area);
}

/// Extract short GPU name from full name
fn short_gpu_name(name: &str) -> String {
    let parts: Vec<&str> = name.split(|c: char| c.is_whitespace() || c == '-').collect();
    if let Some(pos) = parts.iter().position(|&s| {
        s == "RTX" || s == "GTX" || s == "GT" || s == "T4" || s == "A100"
        || s == "A10" || s == "A40" || s == "A30" || s == "V100" || s == "H100"
        || s == "H200" || s == "L40" || s == "L4"
    }) {
        if pos + 1 < parts.len() && parts[pos + 1].chars().next().is_some_and(|c| c.is_ascii_digit()) {
            format!("{}{}", parts[pos], parts[pos + 1])
        } else {
            parts[pos].to_string()
        }
    } else {
        parts.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join("")
    }
}

fn draw_server_list(frame: &mut Frame, app: &App, area: Rect) {
    use std::collections::BTreeMap;

    let filtered = app.filtered_servers();
    let display_order = app.display_order_servers();

    // Get the actual server index that is currently selected (using display order)
    let selected_server_idx = display_order.get(app.selected_index).copied();

    // Group servers by their group name
    let mut grouped: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for &idx in &filtered {
        let group = app.servers[idx].group.clone().unwrap_or_default();
        grouped.entry(group).or_default().push(idx);
    }

    // Build list items with column header, group headers and server rows
    let mut items: Vec<ListItem> = Vec::new();
    let mut flat_index = 0; // Track position for shortcut keys

    // Column header - use same widths as data rows
    let hdr = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let header_line = Line::from(vec![
        Span::styled(format!("{:>3}", "#"), hdr),
        Span::raw("  "),  // Space for star
        Span::styled(format!("{:<13}", "Host"), hdr),
        Span::styled(format!("{:>8}", "Ping"), hdr),
        Span::raw(" "),   // Space for mosh indicator
        Span::styled(format!("{:<14}", "CPU"), hdr),
        Span::styled(format!("{:<13}", "RAM"), hdr),
        Span::styled(format!("{:<18}", "GPU"), hdr),
        Span::styled(format!("{:>5}", "Users"), hdr),
        Span::raw("  "),
        Span::styled(format!("{:<8}", "Last"), hdr),
    ]);
    items.push(ListItem::new(header_line));

    for (group_name, server_indices) in &grouped {
        // Group header
        let header_text = format!("▸ {} ({} servers)", group_name, server_indices.len());
        items.push(ListItem::new(Line::from(vec![
            Span::styled(header_text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ])));

        // Servers in this group
        for &idx in server_indices {
            let server = &app.servers[idx];
            let is_selected = Some(idx) == selected_server_idx;

            // Color code latency: green <100ms, yellow 100-500ms, red >500ms
            let (latency_str, latency_color) = match server.latency_ms() {
                Some(ms) => {
                    let color = if ms <= 100 {
                        Color::Green
                    } else if ms <= 500 {
                        Color::Yellow
                    } else {
                        Color::Red
                    };
                    (format!("{}ms", ms), color)
                }
                None => {
                    let color = match server.status {
                        HealthStatus::Unknown => Color::DarkGray,
                        _ => Color::Red, // Unreachable
                    };
                    ("-".to_string(), color)
                }
            };

            let (cpu_str, ram_str, gpu_str, gpu_color) = if let Some(ref m) = server.metrics {
                let cpu = if m.cpu_cores > 0 {
                    format!("{:>3}% ({:>2}c)", m.cpu_usage as u32, m.cpu_cores)
                } else {
                    format!("{:>3}%", m.cpu_usage as u32)
                };

                let ram = format!("{:>6}/{:<6}", format_bytes(m.ram_used), format_bytes(m.ram_total));

                let (gpu, gpu_util) = if m.gpus.is_empty() {
                    ("-".to_string(), 0.0)
                } else {
                    let count = m.gpus.len();
                    let avg_util: f32 = m.gpus.iter().map(|g| g.utilization).sum::<f32>() / count as f32;
                    let short = short_gpu_name(&m.gpus[0].name);
                    (format!("{}x{} {:>3}%", count, short, avg_util as u32), avg_util)
                };

                let color = if gpu_util > 80.0 {
                    Color::Red
                } else if gpu_util > 50.0 {
                    Color::Yellow
                } else if gpu_util > 0.0 {
                    Color::Green
                } else {
                    Color::DarkGray
                };

                (cpu, ram, gpu, color)
            } else {
                ("-".to_string(), "-".to_string(), "-".to_string(), Color::DarkGray)
            };

            let users_str = server.metrics.as_ref()
                .map(|m| format!("{}", m.logged_in_users.len()))
                .unwrap_or_else(|| "-".to_string());

            // Mosh indicator: M if server has mosh-server
            let mosh_indicator = server.metrics.as_ref()
                .map(|m| if m.has_mosh { "M" } else { " " })
                .unwrap_or(" ");

            // Get last connection time
            let last_str = app.history.format_last_connected(&server.host);

            // Check if favorite
            let is_favorite = app.history.is_favorite(&server.host);
            let fav_indicator = if is_favorite { "★" } else { " " };

            // Generate shortcut key: a-z for first 26, then 0-9
            let shortcut = if flat_index < 26 {
                ((b'a' + flat_index as u8) as char).to_string()
            } else if flat_index < 36 {
                ((b'0' + (flat_index - 26) as u8) as char).to_string()
            } else {
                " ".to_string()
            };

            // Build the line with styled spans - match header widths
            let line = Line::from(vec![
                Span::styled(format!("{:>3}", shortcut), Style::default().fg(Color::DarkGray)),
                Span::styled(format!(" {}", fav_indicator), Style::default().fg(Color::Yellow)),
                Span::styled(format!("{:<13}", server.host), Style::default().fg(Color::White)),
                Span::styled(format!("{:>8}", latency_str), Style::default().fg(latency_color)),
                Span::styled(mosh_indicator, Style::default().fg(Color::Magenta)),
                Span::raw(format!("{:<14}", cpu_str)),
                Span::raw(format!("{:<13}", ram_str)),
                Span::styled(format!("{:<18}", gpu_str), Style::default().fg(gpu_color)),
                Span::styled(format!("{:>5}", users_str), Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(format!("{:<8}", last_str), Style::default().fg(Color::Magenta)),
            ]);

            let style = if is_selected {
                Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            items.push(ListItem::new(line).style(style));
            flat_index += 1;
        }
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Servers "));

    frame.render_widget(list, area);
}

fn draw_group_list(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .groups
        .iter()
        .enumerate()
        .map(|(i, group)| {
            let style = if i == app.selected_group {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let content = format!("{} ({} servers)", group.name, group.servers.len());
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Groups "));

    frame.render_widget(list, area);
}

fn draw_server_details(frame: &mut Frame, app: &App, area: Rect) {
    let server = match app.selected_server() {
        Some(s) => s,
        None => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Server Details ");
            frame.render_widget(Paragraph::new("No server selected").block(block), area);
            return;
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Basic info
            Constraint::Length(6), // System metrics
            Constraint::Min(4),    // GPU / Users
        ])
        .split(area);

    // Basic info - color code latency
    let (latency_str, latency_color) = match server.latency_ms() {
        Some(ms) => {
            let color = if ms <= 100 {
                Color::Green
            } else if ms <= 500 {
                Color::Yellow
            } else {
                Color::Red
            };
            (format!("{}ms", ms), color)
        }
        None => {
            let color = match server.status {
                HealthStatus::Unknown => Color::DarkGray,
                _ => Color::Red,
            };
            ("N/A".to_string(), color)
        }
    };

    let status_color = match server.status {
        HealthStatus::Healthy => Color::Green,
        HealthStatus::Degraded => Color::Yellow,
        HealthStatus::Unreachable => Color::Red,
        HealthStatus::Unknown => Color::DarkGray,
    };

    let info_lines = vec![
        Line::from(vec![
            Span::raw("Host:     "),
            Span::styled(&server.host, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("Hostname: "),
            Span::styled(&server.hostname, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::raw("User:     "),
            Span::styled(
                server.user.as_deref().unwrap_or("(default)"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::raw("Port:     "),
            Span::styled(server.port.to_string(), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::raw("Status:   "),
            Span::styled(
                format!("{:?}", server.status),
                Style::default().fg(status_color),
            ),
        ]),
        Line::from(vec![
            Span::raw("Latency:  "),
            Span::styled(latency_str, Style::default().fg(latency_color)),
        ]),
    ];

    // Add mosh availability if we have metrics
    let mosh_line = server.metrics.as_ref().map(|m| {
        let (mosh_str, mosh_color) = if m.has_mosh {
            ("Available", Color::Green)
        } else {
            ("Not installed", Color::DarkGray)
        };
        Line::from(vec![
            Span::raw("Mosh:     "),
            Span::styled(mosh_str, Style::default().fg(mosh_color)),
        ])
    });

    let mut all_lines = info_lines;
    if let Some(line) = mosh_line {
        all_lines.push(line);
    }

    let info_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", server.host));
    frame.render_widget(Paragraph::new(all_lines).block(info_block), chunks[0]);

    // System metrics
    if let Some(ref metrics) = server.metrics {
        let metrics_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        // CPU gauge
        let cpu_gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title(" CPU "))
            .gauge_style(Style::default().fg(gauge_color(metrics.cpu_usage)))
            .percent(metrics.cpu_usage as u16)
            .label(format!("{:.1}%", metrics.cpu_usage));
        frame.render_widget(cpu_gauge, metrics_chunks[0]);

        // RAM gauge
        let ram_percent = metrics.ram_usage_percent();
        let ram_label = format!(
            "{} / {} ({:.1}%)",
            format_bytes(metrics.ram_used),
            format_bytes(metrics.ram_total),
            ram_percent
        );
        let ram_gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title(" RAM "))
            .gauge_style(Style::default().fg(gauge_color(ram_percent)))
            .percent(ram_percent as u16)
            .label(ram_label);
        frame.render_widget(ram_gauge, metrics_chunks[1]);

        // GPU and users
        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(chunks[2]);

        // GPU info
        let gpu_lines: Vec<Line> = if metrics.gpus.is_empty() {
            vec![Line::from("No GPU detected")]
        } else {
            metrics
                .gpus
                .iter()
                .map(|gpu| {
                    Line::from(format!(
                        "{}: {}% | {} / {}",
                        gpu.name,
                        gpu.utilization,
                        format_bytes(gpu.memory_used),
                        format_bytes(gpu.memory_total)
                    ))
                })
                .collect()
        };
        let gpu_block = Block::default().borders(Borders::ALL).title(" GPUs ");
        frame.render_widget(Paragraph::new(gpu_lines).block(gpu_block), bottom_chunks[0]);

        // Users
        let users_text = if metrics.logged_in_users.is_empty() {
            "No users logged in".to_string()
        } else {
            metrics.logged_in_users.join(", ")
        };
        let users_block = Block::default().borders(Borders::ALL).title(" Users ");
        frame.render_widget(
            Paragraph::new(users_text)
                .wrap(Wrap { trim: true })
                .block(users_block),
            bottom_chunks[1],
        );
    } else {
        let no_metrics = Paragraph::new("No metrics available. Press 'r' to refresh.")
            .block(Block::default().borders(Borders::ALL).title(" Metrics "));
        frame.render_widget(no_metrics, chunks[1]);
    }
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from(vec![Span::styled(
            "Navigation",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  j/↓       Move down"),
        Line::from("  k/↑       Move up"),
        Line::from("  a-z, 0-9  Quick connect to server"),
        Line::from("  Enter     Connect to selected server"),
        Line::from("  d/Space   Show server details"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Search",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  /         Search/filter servers (regex)"),
        Line::from("  n         Next match"),
        Line::from("  N         Previous match"),
        Line::from("  Esc       Clear search"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Views",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  G         Switch to group view"),
        Line::from("  Esc       Back to server list"),
        Line::from("  ?         Toggle help"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Actions",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  c         Run command on server"),
        Line::from("  f         Toggle favorite ★"),
        Line::from("  s         Cycle sort order"),
        Line::from("  r         Refresh all servers"),
        Line::from("  R         Refresh selected server"),
        Line::from("  m         Toggle mosh/ssh mode"),
        Line::from("  M         Mosh install menu"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Tunnels",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  t         Open SSH tunnel (port, range, or host:port)"),
        Line::from("  T         View active tunnels"),
        Line::from("  d/Del     Close selected tunnel (in tunnel view)"),
        Line::from("  D         Close all tunnels (in tunnel view)"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Command Output",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  y         Copy output to clipboard"),
        Line::from("  >         Save output to file"),
        Line::from("  |         Pipe output to local command"),
        Line::from(""),
        Line::from("  q         Quit"),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help ")
        .style(Style::default().fg(Color::White));

    frame.render_widget(Paragraph::new(help_text).block(block), area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL);

    if let Some(ref msg) = app.status_message {
        let paragraph = Paragraph::new(msg.as_str())
            .style(Style::default().fg(Color::Yellow))
            .block(block);
        frame.render_widget(paragraph, area);
    } else if app.is_fetching {
        let paragraph = Paragraph::new("Fetching server metrics...")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(paragraph, area);
    } else {
        // Show styled key hints based on view mode
        let mosh_indicator = if app.use_mosh {
            Span::styled("[MOSH]", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("[SSH]", Style::default().fg(Color::Cyan))
        };

        let hints = match app.view_mode {
            ViewMode::ServerList => Line::from(vec![
                Span::raw(" "),
                mosh_indicator,
                Span::raw("  "),
                Span::styled("?", Style::default().fg(Color::Yellow)),
                Span::raw(":help  "),
                Span::styled("/", Style::default().fg(Color::Yellow)),
                Span::raw(":filter  "),
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw(":connect  "),
                Span::styled("c", Style::default().fg(Color::Yellow)),
                Span::raw(":cmd  "),
                Span::styled("t", Style::default().fg(Color::Yellow)),
                Span::raw(":tunnel  "),
                Span::styled("m", Style::default().fg(Color::Yellow)),
                Span::raw(":mosh  "),
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(":quit"),
            ]),
            ViewMode::GroupList => Line::from(vec![
                Span::styled(" Enter", Style::default().fg(Color::Yellow)),
                Span::raw(":select  "),
                Span::styled("Esc", Style::default().fg(Color::Yellow)),
                Span::raw(":back  "),
                Span::styled("?", Style::default().fg(Color::Yellow)),
                Span::raw(":help"),
            ]),
            ViewMode::ServerDetails => Line::from(vec![
                Span::styled(" Enter", Style::default().fg(Color::Yellow)),
                Span::raw(":connect  "),
                Span::styled("r", Style::default().fg(Color::Yellow)),
                Span::raw(":refresh  "),
                Span::styled("j/k", Style::default().fg(Color::Yellow)),
                Span::raw(":nav  "),
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(":back"),
            ]),
            ViewMode::CommandOutput => Line::from(vec![
                Span::styled(" y", Style::default().fg(Color::Yellow)),
                Span::raw(":copy  "),
                Span::styled(">", Style::default().fg(Color::Yellow)),
                Span::raw(":save  "),
                Span::styled("|", Style::default().fg(Color::Yellow)),
                Span::raw(":pipe  "),
                Span::styled("c", Style::default().fg(Color::Yellow)),
                Span::raw(":new cmd  "),
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(":back"),
            ]),
            ViewMode::Tunnels | ViewMode::Help => Line::from(vec![
                Span::styled(" q", Style::default().fg(Color::Yellow)),
                Span::raw(":back"),
            ]),
        };
        frame.render_widget(Paragraph::new(hints).block(block), area);
    }
}

fn draw_filter_input(frame: &mut Frame, app: &App) {
    let area = constrained_rect(frame.area(), MAX_WIDTH);
    let popup_width = area.width.min(60);
    let popup_area = Rect {
        x: area.x + (area.width - popup_width) / 2,
        y: area.height / 2 - 2,
        width: popup_width,
        height: 3,
    };

    frame.render_widget(Clear, popup_area);

    let input = Paragraph::new(format!("/{}", app.filter_text))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Filter ")
                .style(Style::default().fg(Color::Cyan)),
        );

    frame.render_widget(input, popup_area);
}

fn draw_command_input(frame: &mut Frame, app: &App) {
    let area = constrained_rect(frame.area(), MAX_WIDTH);
    let popup_width = area.width.min(70);

    // Get server name for title
    let server_name = app
        .selected_server()
        .map(|s| s.host.as_str())
        .unwrap_or("?");

    let popup_area = Rect {
        x: area.x + (area.width - popup_width) / 2,
        y: area.height / 2 - 2,
        width: popup_width,
        height: 3,
    };

    frame.render_widget(Clear, popup_area);

    let input = Paragraph::new(format!("$ {}", app.command_text))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Run on {} ", server_name))
                .style(Style::default().fg(Color::Green)),
        );

    frame.render_widget(input, popup_area);
}

fn draw_command_output(frame: &mut Frame, app: &App, area: Rect) {
    let server = app.command_server.as_deref().unwrap_or("?");
    let title = format!(" Output from {} ", server);

    // Split area for output and hints
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let output_text = if app.is_running_command {
        "Running command...".to_string()
    } else {
        app.command_output
            .clone()
            .unwrap_or_else(|| "No output".to_string())
    };

    let paragraph = Paragraph::new(output_text)
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().fg(Color::Cyan)),
        );

    frame.render_widget(paragraph, chunks[0]);

    // Show hints for actions
    let hints = Line::from(vec![
        Span::styled(" y", Style::default().fg(Color::Yellow)),
        Span::raw(":copy  "),
        Span::styled(">", Style::default().fg(Color::Yellow)),
        Span::raw(":save  "),
        Span::styled("|", Style::default().fg(Color::Yellow)),
        Span::raw(":pipe  "),
        Span::styled("c", Style::default().fg(Color::Yellow)),
        Span::raw(":cmd  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(":back"),
    ]);
    frame.render_widget(Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)), chunks[1]);
}

fn draw_pipe_input(frame: &mut Frame, app: &App) {
    let area = constrained_rect(frame.area(), MAX_WIDTH);
    let popup_width = area.width.min(70);

    let popup_area = Rect {
        x: area.x + (area.width - popup_width) / 2,
        y: area.height / 2 - 2,
        width: popup_width,
        height: 3,
    };

    frame.render_widget(Clear, popup_area);

    let input = Paragraph::new(format!("| {}", app.pipe_text))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Pipe to local command ")
                .style(Style::default().fg(Color::Magenta)),
        );

    frame.render_widget(input, popup_area);
}

fn draw_save_input(frame: &mut Frame, app: &App) {
    let area = constrained_rect(frame.area(), MAX_WIDTH);
    let popup_width = area.width.min(70);

    let popup_area = Rect {
        x: area.x + (area.width - popup_width) / 2,
        y: area.height / 2 - 2,
        width: popup_width,
        height: 3,
    };

    frame.render_widget(Clear, popup_area);

    let input = Paragraph::new(format!("> {}", app.save_path))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Save to file ")
                .style(Style::default().fg(Color::Blue)),
        );

    frame.render_widget(input, popup_area);
}

fn draw_tunnel_input(frame: &mut Frame, app: &App) {
    let area = constrained_rect(frame.area(), MAX_WIDTH);
    let popup_width = area.width.min(70);

    let server_name = app
        .selected_server()
        .map(|s| s.host.as_str())
        .unwrap_or("?");

    let popup_area = Rect {
        x: area.x + (area.width - popup_width) / 2,
        y: area.height / 2 - 3,
        width: popup_width,
        height: 5,
    };

    frame.render_widget(Clear, popup_area);

    let hint = "Format: [host:]port (e.g., 8080, localhost:3000)";
    let text = vec![
        Line::from(hint).style(Style::default().fg(Color::DarkGray)),
        Line::from(format!("→ {}", app.tunnel_input)).style(Style::default().fg(Color::White)),
    ];

    let input = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Open tunnel via {} ", server_name))
            .style(Style::default().fg(Color::Magenta)),
    );

    frame.render_widget(input, popup_area);
}

fn draw_tunnels(frame: &mut Frame, app: &App, area: Rect) {
    let display_items = app.tunnel_manager.get_display_items();

    if display_items.is_empty() {
        let text = Paragraph::new("No active tunnels.\n\nPress 't' on a server to open a tunnel.")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Tunnels ")
                    .style(Style::default().fg(Color::Cyan)),
            );
        frame.render_widget(text, area);
        return;
    }

    // Split area for list and hints
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let mut items: Vec<ListItem> = Vec::new();

    // Header
    let hdr = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let header_line = Line::from(vec![
        Span::styled(format!("{:<15}", "Local"), hdr),
        Span::raw("  "),
        Span::styled(format!("{:<25}", "Remote"), hdr),
        Span::raw("  "),
        Span::styled(format!("{:<15}", "Via Server"), hdr),
    ]);
    items.push(ListItem::new(header_line));

    for (i, item) in display_items.iter().enumerate() {
        let is_selected = i == app.selected_tunnel;

        let line = match item {
            TunnelDisplayItem::Single {
                local_port,
                remote_host,
                remote_port,
                server_host,
            } => Line::from(vec![
                Span::styled(
                    format!("{:<15}", format!(":{}", local_port)),
                    Style::default().fg(Color::Green),
                ),
                Span::raw("→ "),
                Span::styled(
                    format!("{:<25}", format!("{}:{}", remote_host, remote_port)),
                    Style::default().fg(Color::White),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{:<15}", server_host),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            TunnelDisplayItem::Group {
                local_port_start,
                local_port_end,
                remote_host,
                remote_port_start,
                remote_port_end,
                server_host,
                count,
                ..
            } => Line::from(vec![
                Span::styled(
                    format!("{:<15}", format!(":{}-{}", local_port_start, local_port_end)),
                    Style::default().fg(Color::Green),
                ),
                Span::raw("→ "),
                Span::styled(
                    format!(
                        "{:<25}",
                        format!("{}:{}-{}", remote_host, remote_port_start, remote_port_end)
                    ),
                    Style::default().fg(Color::White),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{:<15}", server_host),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!(" ({})", count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        };

        let style = if is_selected {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        items.push(ListItem::new(line).style(style));
    }

    let total_tunnels = app.tunnel_manager.count();
    let title = format!(" Active Tunnels ({}) ", total_tunnels);
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

    frame.render_widget(list, chunks[0]);

    // Hints
    let hints = Line::from(vec![
        Span::styled(" d", Style::default().fg(Color::Yellow)),
        Span::raw(":close  "),
        Span::styled("D", Style::default().fg(Color::Yellow)),
        Span::raw(":close all  "),
        Span::styled("t", Style::default().fg(Color::Yellow)),
        Span::raw(":new  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(":back"),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

fn gauge_color(percent: f32) -> Color {
    if percent < 50.0 {
        Color::Green
    } else if percent < 80.0 {
        Color::Yellow
    } else {
        Color::Red
    }
}

fn draw_install_menu(frame: &mut Frame, app: &App) {
    let area = constrained_rect(frame.area(), MAX_WIDTH);
    let popup_width = 50;
    let popup_height = 10;

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    frame.render_widget(Clear, popup_area);

    // Get selected server name for the menu
    let server_name = app
        .selected_server()
        .map(|s| s.host.as_str())
        .unwrap_or("(none)");

    let menu_items = [
        ("1", "Install mosh locally"),
        ("2", &format!("Install on {}", server_name)),
        ("3", "Install on all servers"),
        ("4", "Show install instructions"),
    ];

    let items: Vec<Line> = menu_items
        .iter()
        .enumerate()
        .map(|(i, (key, label))| {
            let is_selected = i == app.install_menu_selection;
            let prefix = if is_selected { "▸ " } else { "  " };
            let style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("[{}] ", key), Style::default().fg(Color::Yellow)),
                Span::styled(*label, style),
            ])
        })
        .collect();

    let mut lines = vec![
        Line::from(""),
    ];
    lines.extend(items);
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  j/k", Style::default().fg(Color::DarkGray)),
        Span::raw(": navigate  "),
        Span::styled("Enter", Style::default().fg(Color::DarkGray)),
        Span::raw(": select  "),
        Span::styled("Esc", Style::default().fg(Color::DarkGray)),
        Span::raw(": cancel"),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Install Mosh ")
        .style(Style::default().fg(Color::Magenta));

    frame.render_widget(Paragraph::new(lines).block(block), popup_area);
}
