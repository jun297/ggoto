<h1 align="center">
  <br>
  ggoto
  <br>
</h1>

<h4 align="center">A blazingly fast TUI for managing SSH connections with real-time server health monitoring.</h4>

<p align="center">
  <a href="https://github.com/jiwan/ggoto/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="License">
  </a>
  <img src="https://img.shields.io/badge/rust-1.70%2B-orange.svg?style=flat-square" alt="Rust Version">
</p>

<p align="center">
  <a href="#-features">Features</a> •
  <a href="#-installation">Installation</a> •
  <a href="#-usage">Usage</a> •
  <a href="#-key-bindings">Key Bindings</a> •
  <a href="#-configuration">Configuration</a> •
  <a href="#-contributing">Contributing</a>
</p>

---

<p align="center">
  <img src="assets/demo.gif" alt="ggoto demo" width="800">
</p>

## ✨ Features

- **Zero Configuration** — Automatically reads your `~/.ssh/config` file
- **Real-time Health Monitoring** — Live CPU, RAM, and latency metrics for all servers
- **Smart Server Grouping** — Automatically groups servers by naming patterns (e.g., `prod-web-01`, `prod-web-02` → `prod-web`)
- **Fuzzy Search & Regex Filtering** — Quickly find servers with `/` search supporting regex patterns
- **SSH Tunneling** — Open and manage SSH tunnels with port ranges (e.g., `8000-8010`)
- **Remote Command Execution** — Run commands on servers without full SSH sessions
- **Favorites & History** — Mark favorite servers with ★ and track connection history
- **Multiple Sort Options** — Sort by name, latency, CPU, RAM, favorites, or recent usage
- **Clipboard & Pipe Support** — Copy command output or pipe to local commands
- **Fully Async** — Built on Tokio for non-blocking operations

## 📦 Installation

### Using Cargo (Recommended)

```bash
cargo install --git https://github.com/jiwan/ggoto.git
```

### From Source

```bash
git clone https://github.com/jiwan/ggoto.git
cd ggoto
cargo build --release

# Binary will be at target/release/ggoto
```

### From Releases

Download the latest binary from the [Releases](https://github.com/jiwan/ggoto/releases) page.

### Requirements

- Rust 1.70+ (for building)
- SSH client installed and configured

## 🚀 Usage

Simply run:

```bash
ggoto
```

ggoto will automatically parse your `~/.ssh/config` and display all configured hosts.

### Quick Connect

Use shortcut keys `a-z` and `0-9` to instantly connect to servers (shown next to each server name).

## ⌨️ Key Bindings

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | Connect to selected server |
| `a-z`, `0-9` | Quick connect to server |
| `d` / `Space` | Show server details |
| `G` | Switch to group view |
| `Home` / `End` | Jump to first/last |
| `PgUp` / `PgDn` | Page up/down |

### Search & Filter

| Key | Action |
|-----|--------|
| `/` | Start search/filter (supports regex) |
| `n` | Next search result |
| `N` | Previous search result |
| `Esc` | Clear search |

### Actions

| Key | Action |
|-----|--------|
| `c` | Run command on selected server |
| `f` | Toggle favorite ★ |
| `s` | Cycle sort order |
| `r` | Refresh all servers |
| `R` | Refresh selected server |

### Tunnels

| Key | Action |
|-----|--------|
| `t` | Open SSH tunnel |
| `T` | View active tunnels |
| `d` | Close selected tunnel (in tunnel view) |
| `D` | Close all tunnels (in tunnel view) |

### Command Output

| Key | Action |
|-----|--------|
| `y` | Copy output to clipboard |
| `>` | Save output to file |
| `\|` | Pipe output to local command |

### General

| Key | Action |
|-----|--------|
| `?` | Show help |
| `q` / `Esc` | Quit / Go back |
| `Ctrl+C` | Force quit |

## 🔗 SSH Tunnels

Open tunnels with flexible syntax:

```bash
# Single port (localhost:3306 on remote)
t → 3306

# Specific remote host
t → db.internal:5432

# Port range (opens 11 tunnels)
t → 8000-8010

# Remote host with port range
t → redis.internal:6379-6384
```

Tunnels opened as a range are grouped together in the tunnel view:

```
┌─────────────────────────────────────────────────────────────────┐
│  Active Tunnels (11)                                            │
├─────────────────────────────────────────────────────────────────┤
│  Local            Remote                     Via Server         │
│  :8000-8010    →  localhost:8000-8010        prod-web-01  (11)  │
│  :3306         →  db.internal:3306           prod-db-01         │
└─────────────────────────────────────────────────────────────────┘
```

## 📊 Sort Orders

Cycle through with `s`:

| Order | Description |
|-------|-------------|
| **Name** | Alphabetical by hostname |
| **Favorites** | Starred servers first ★ |
| **Recently Used** | Most recently connected first |
| **Latency** | Fastest response time first |
| **CPU Usage** | Lowest CPU usage first |
| **RAM Usage** | Lowest RAM usage first |
| **Group** | Grouped by server group |

## ⚙️ Configuration

### Data Storage

ggoto stores data in `~/.config/ggoto/`:

```
~/.config/ggoto/
└── history.json    # Connection history, favorites, sort preference
```

### SSH Config

ggoto reads standard SSH config format:

```ssh-config
# ~/.ssh/config

Host prod-web-01
    HostName 10.0.1.1
    User deploy
    Port 22
    IdentityFile ~/.ssh/prod_key

Host prod-web-02
    HostName 10.0.1.2
    User deploy

Host prod-db-*
    User admin
    ForwardAgent yes

Host dev-server
    HostName dev.example.com
    User developer
```

Servers are automatically grouped by naming patterns:
- `prod-web-01`, `prod-web-02` → group `prod-web`
- `prod-db-01` → group `prod-db`

## 📈 Health Metrics

ggoto collects real-time metrics from each server:

| Metric | Source |
|--------|--------|
| Latency | SSH connection time |
| CPU Usage | `/proc/stat` |
| RAM Usage | `/proc/meminfo` |
| Load Average | `/proc/loadavg` |
| Logged-in Users | `who` command |

### Latency Color Coding

| Color | Latency |
|-------|---------|
| 🟢 Green | < 100ms |
| 🟡 Yellow | 100-500ms |
| 🔴 Red | > 500ms |

## 🏗️ Project Structure

```
src/
├── main.rs           # Entry point, event loop
├── app.rs            # Application state management
├── server.rs         # Server and group data structures
├── health.rs         # Async health check logic
├── history.rs        # Connection history & favorites
├── tunnel.rs         # SSH tunnel management
├── ssh/
│   ├── mod.rs
│   ├── config.rs     # SSH config parsing
│   └── connection.rs # SSH session management
└── tui/
    ├── mod.rs
    ├── ui.rs         # UI rendering
    └── event.rs      # Input handling
```

## 🛠️ Development

```bash
# Build
cargo build

# Run in development
cargo run

# Run tests
cargo test

# Run linter
cargo clippy -- -D warnings

# Format code
cargo fmt

# Release build
cargo build --release
```

## 🗺️ Roadmap

- [ ] GPU monitoring (NVIDIA/AMD)
- [ ] Custom health check commands
- [ ] Server tags and custom grouping
- [ ] Connection multiplexing
- [ ] SOCKS proxy support
- [ ] Theme customization
- [ ] Config file for preferences

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built with [Ratatui](https://github.com/ratatui-org/ratatui) for the beautiful TUI
- Async runtime powered by [Tokio](https://tokio.rs/)
- Inspired by [lazyssh](https://github.com/xxxserxxx/lazyssh) and [sshs](https://github.com/quantumsheep/sshs)

---

<p align="center">
  <sub>Built with ❤️ and Rust</sub>
</p>
