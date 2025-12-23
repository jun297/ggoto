# CLAUDE.md for SSH-TUI-Rust

## Project Overview

A high-performance Rust CLI/TUI application for managing SSH connections. The tool focuses on low-latency server monitoring (CPU/RAM/GPU), SSH config parsing, and intelligent server grouping to streamline job launching.

## Build & Development Commands

* **Build:** `cargo build`
* **Run:** `cargo run`
* **Test:** `cargo test`
* **Lint:** `cargo clippy -- -D warnings`
* **Format:** `cargo fmt`
* **Release Build:** `cargo build --release`

## Core Features & Requirements

1. **SSH Integration:** Seamlessly initiate SSH sessions.
2. **Config Parsing:** Automatically read `~/.ssh/config` to populate server candidates.
3. **Smart Grouping:** Logic to group servers based on naming conventions (e.g., `prod-web-01`, `prod-web-02` → `prod-web`).
4. **Health Metrics:** Real-time latency checks and system info (CPU, RAM, and NVIDIA/AMD GPU utilization).
5. **Occupancy Check:** Identify currently logged-in users or active processes per server.
6. **Quick Launch:** Filter-and-select UI to launch jobs on specific groups or servers meeting criteria (e.g., "lowest RAM usage").
7. **TUI Aesthetic:** Modern, high-performance UI using `ratatui`.

## Technical Stack

* **Language:** Rust (Latest Stable)
* **TUI Framework:** `ratatui` with `crossterm` backend.
* **Async Runtime:** `tokio` (essential for parallel latency/sysinfo checks).
* **SSH Logic:** `ssh2-rs` or wrapping native `ssh` commands.
* **Parsing:** `ssh-config` crate for handling local configurations.

## Code Style & Patterns

* **Error Handling:** Use `anyhow` for application-level errors and `thiserror` for library-level errors.
* **Concurrency:** Use `tokio::spawn` for multi-server health checks to ensure the TUI remains responsive.
* **State Management:** Follow a centralized `App` struct pattern to hold TUI state, server lists, and terminal dimensions.
* **Performance:** Favor stack-allocated data where possible; avoid unnecessary clones in the render loop.
* **Naming:** * Methods fetching remote data: `fetch_*` (async).
* Methods updating local state: `update_*`.
* UI components: `draw_*`.



---

### How to use this file

1. Place this in your **project root**.
2. When starting a new chat with an AI, it will read this to know that it should use **Rust**, focus on **Ratatui** for the UI, and prioritize **Asynchronous I/O** for the server pings.
