use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Health status of a server
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum HealthStatus {
    #[default]
    Unknown,
    Healthy,
    Degraded,
    Unreachable,
}

/// GPU information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub utilization: f32,
    pub memory_used: u64,
    pub memory_total: u64,
}

/// System metrics fetched from a remote server
#[derive(Debug, Clone, Default)]
pub struct SystemMetrics {
    pub cpu_cores: u32,
    pub cpu_usage: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    pub gpus: Vec<GpuInfo>,
    pub logged_in_users: Vec<String>,
    pub load_average: (f32, f32, f32),
}

impl SystemMetrics {
    pub fn ram_usage_percent(&self) -> f32 {
        if self.ram_total == 0 {
            0.0
        } else {
            (self.ram_used as f32 / self.ram_total as f32) * 100.0
        }
    }
}

/// Represents an SSH server from the config
#[derive(Debug, Clone)]
pub struct Server {
    pub host: String,
    pub hostname: String,
    pub user: Option<String>,
    pub port: u16,
    pub identity_file: Option<String>,
    pub group: Option<String>,

    // Health and metrics
    pub latency: Option<Duration>,
    pub status: HealthStatus,
    pub metrics: Option<SystemMetrics>,
    pub last_check: Option<std::time::Instant>,
}

impl Server {
    pub fn new(host: String, hostname: String) -> Self {
        Self {
            host,
            hostname,
            user: None,
            port: 22,
            identity_file: None,
            group: None,
            latency: None,
            status: HealthStatus::Unknown,
            metrics: None,
            last_check: None,
        }
    }

    /// Get latency in milliseconds
    pub fn latency_ms(&self) -> Option<u64> {
        self.latency.map(|d| d.as_millis() as u64)
    }

    /// Check if metrics are stale (older than 30 seconds)
    #[allow(dead_code)]
    pub fn metrics_stale(&self) -> bool {
        match self.last_check {
            Some(t) => t.elapsed() > Duration::from_secs(30),
            None => true,
        }
    }
}

/// A group of servers with a common prefix
#[derive(Debug, Clone)]
pub struct ServerGroup {
    pub name: String,
    pub servers: Vec<usize>, // Indices into the main server list
}

impl ServerGroup {
    pub fn new(name: String) -> Self {
        Self {
            name,
            servers: Vec::new(),
        }
    }
}

/// Generate demo servers with fake data for screenshots/demos
pub fn generate_demo_servers() -> Vec<Server> {
    use std::time::Duration;

    let demo_data = [
        ("prod-web-01", "10.0.1.1", "deploy", 12, 23.0, 4_200_000_000u64, 8_000_000_000u64),
        ("prod-web-02", "10.0.1.2", "deploy", 15, 45.0, 3_800_000_000, 8_000_000_000),
        ("prod-web-03", "10.0.1.3", "deploy", 18, 67.0, 5_100_000_000, 8_000_000_000),
        ("prod-db-01", "10.0.2.1", "admin", 8, 12.0, 8_100_000_000, 16_000_000_000),
        ("prod-db-02", "10.0.2.2", "admin", 9, 15.0, 7_800_000_000, 16_000_000_000),
        ("staging-api", "staging.example.com", "developer", 45, 5.0, 2_100_000_000, 4_000_000_000),
        ("staging-web", "staging-web.example.com", "developer", 48, 8.0, 1_800_000_000, 4_000_000_000),
        ("dev-server", "dev.example.com", "dev", 120, 67.0, 1_200_000_000, 2_000_000_000),
        ("ci-runner-01", "ci-01.internal", "ci", 25, 89.0, 3_500_000_000, 4_000_000_000),
        ("ci-runner-02", "ci-02.internal", "ci", 28, 45.0, 2_800_000_000, 4_000_000_000),
        ("monitoring", "monitor.example.com", "ops", 35, 15.0, 1_500_000_000, 2_000_000_000),
        ("bastion", "bastion.example.com", "admin", 5, 2.0, 500_000_000, 1_000_000_000),
    ];

    demo_data
        .iter()
        .map(|(host, hostname, user, latency_ms, cpu, ram_used, ram_total)| {
            let mut server = Server::new(host.to_string(), hostname.to_string());
            server.user = Some(user.to_string());
            server.latency = Some(Duration::from_millis(*latency_ms));
            server.status = if *latency_ms < 100 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Degraded
            };
            server.metrics = Some(SystemMetrics {
                cpu_cores: 4,
                cpu_usage: *cpu,
                ram_used: *ram_used,
                ram_total: *ram_total,
                gpus: vec![],
                logged_in_users: vec!["user".to_string()],
                load_average: (cpu / 25.0, cpu / 30.0, cpu / 35.0),
            });
            server.last_check = Some(std::time::Instant::now());
            server
        })
        .collect()
}
