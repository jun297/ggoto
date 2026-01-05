use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::{mpsc, Semaphore};

use crate::server::{GpuInfo, HealthStatus, Server, SystemMetrics};
use crate::ssh::connection::run_remote_command;
use crate::ssh::mosh::is_mosh_installed;

/// Maximum concurrent health check connections
const MAX_CONCURRENT_CHECKS: usize = 5;

/// Message sent from health check tasks
#[derive(Debug)]
pub struct HealthUpdate {
    pub server_idx: usize,
    pub latency: Option<Duration>,
    pub status: HealthStatus,
    pub metrics: Option<SystemMetrics>,
}

/// Check latency to a server using SSH
pub async fn check_latency(server: &Server) -> Option<Duration> {
    let start = Instant::now();

    // Try to run a simple command to measure round-trip time
    let result = run_remote_command(server, "echo ok").await;

    if result.is_ok() {
        Some(start.elapsed())
    } else {
        None
    }
}

/// Fetch system metrics from a server
pub async fn fetch_metrics(server: &Server) -> Result<SystemMetrics> {
    // Combined command to fetch all metrics at once
    let base_script = r#"
echo "===CORES==="
nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo "0"

echo "===CPU==="
top -bn1 | head -5 | grep 'Cpu' | awk '{print $2}' | tr -d '%us,' 2>/dev/null || \
    top -l1 | grep 'CPU usage' | awk '{print $3}' | tr -d '%' 2>/dev/null || \
    echo "0"

echo "===MEM==="
free -b 2>/dev/null | awk '/^Mem:/ {print $2, $3}' || \
    vm_stat 2>/dev/null | awk '/Pages (free|active|inactive|wired)/ {sum += $NF} END {print sum * 4096}' || \
    echo "0 0"

echo "===LOAD==="
uptime | awk -F'load average:' '{print $2}' | tr -d ' ' 2>/dev/null || echo "0,0,0"

echo "===USERS==="
who | awk '{print $1}' | sort -u 2>/dev/null || echo ""

echo "===GPU==="
nvidia-smi --query-gpu=name,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null || \
    rocm-smi --showuse --showmemuse 2>/dev/null | grep -E 'GPU|Memory' || \
    echo ""
"#;

    // Only check for mosh-server if mosh is installed locally
    let script = if is_mosh_installed() {
        format!(
            r#"{}
echo "===MOSH==="
which mosh-server >/dev/null 2>&1 && echo "yes" || echo "no"
"#,
            base_script
        )
    } else {
        base_script.to_string()
    };

    let output = run_remote_command(server, &script).await?;
    parse_metrics_output(&output)
}

/// Parse the output from our metrics script
fn parse_metrics_output(output: &str) -> Result<SystemMetrics> {
    let mut metrics = SystemMetrics::default();
    let mut section = "";

    for line in output.lines() {
        let line = line.trim();

        if line.starts_with("===") {
            section = line.trim_matches('=');
            continue;
        }

        match section {
            "CORES" => {
                if let Ok(cores) = line.parse::<u32>() {
                    metrics.cpu_cores = cores;
                }
            }
            "CPU" => {
                if let Ok(cpu) = line.parse::<f32>() {
                    metrics.cpu_usage = cpu;
                }
            }
            "MEM" => {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    metrics.ram_total = parts[0].parse().unwrap_or(0);
                    metrics.ram_used = parts[1].parse().unwrap_or(0);
                }
            }
            "LOAD" => {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 3 {
                    metrics.load_average = (
                        parts[0].trim().parse().unwrap_or(0.0),
                        parts[1].trim().parse().unwrap_or(0.0),
                        parts[2].trim().parse().unwrap_or(0.0),
                    );
                }
            }
            "USERS" => {
                if !line.is_empty() {
                    metrics.logged_in_users.push(line.to_string());
                }
            }
            "GPU" => {
                if !line.is_empty() && !line.starts_with("rocm") {
                    // Parse NVIDIA format: name, util%, mem_used, mem_total
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 4 {
                        metrics.gpus.push(GpuInfo {
                            name: parts[0].to_string(),
                            utilization: parts[1].parse().unwrap_or(0.0),
                            memory_used: parts[2].parse::<u64>().unwrap_or(0) * 1024 * 1024,
                            memory_total: parts[3].parse::<u64>().unwrap_or(0) * 1024 * 1024,
                        });
                    }
                }
            }
            "MOSH" => {
                metrics.has_mosh = line == "yes";
            }
            _ => {}
        }
    }

    Ok(metrics)
}

/// Latency threshold in milliseconds (>100ms = degraded)
const LATENCY_GOOD_MS: u64 = 100;

/// Spawn a health check task for a single server (no concurrency limit)
pub fn spawn_health_check(
    server_idx: usize,
    server: Server,
    tx: mpsc::UnboundedSender<HealthUpdate>,
) {
    spawn_health_check_with_semaphore(server_idx, server, tx, None);
}

/// Spawn a health check task with optional semaphore for concurrency limiting
fn spawn_health_check_with_semaphore(
    server_idx: usize,
    server: Server,
    tx: mpsc::UnboundedSender<HealthUpdate>,
    semaphore: Option<Arc<Semaphore>>,
) {
    tokio::spawn(async move {
        // Acquire semaphore permit if provided (limits concurrent SSH connections)
        let _permit = if let Some(ref sem) = semaphore {
            Some(sem.acquire().await)
        } else {
            None
        };

        // Check latency first
        let latency = check_latency(&server).await;
        let status = match latency {
            Some(d) => {
                let ms = d.as_millis() as u64;
                if ms <= LATENCY_GOOD_MS {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Degraded // Reachable but slow
                }
            }
            None => HealthStatus::Unreachable,
        };

        // If reachable, fetch metrics
        let metrics = if status != HealthStatus::Unreachable {
            fetch_metrics(&server).await.ok()
        } else {
            None
        };

        let _ = tx.send(HealthUpdate {
            server_idx,
            latency,
            status,
            metrics,
        });

        // Permit is dropped here, allowing another task to proceed
    });
}

/// Spawn health checks for all servers with concurrency limiting
pub fn spawn_all_health_checks(servers: &[Server], tx: mpsc::UnboundedSender<HealthUpdate>) {
    // Use a semaphore to limit concurrent SSH connections
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS));

    for (idx, server) in servers.iter().enumerate() {
        spawn_health_check_with_semaphore(idx, server.clone(), tx.clone(), Some(semaphore.clone()));
    }
}

/// Format bytes to human-readable string (rounded to integers)
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.0}TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.0}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}
