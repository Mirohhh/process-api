// sender/src/main.rs
//
// Process sender service:
// - Samples local process information every 2 seconds
// - Builds a dashboard payload for the current host
// - Sends payload to the dashboard API endpoint

use dashboard_shared::{DashboardPayload, ProcessInfo};
use std::error::Error;
use sysinfo::{Process, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::time::{sleep, Duration};

/// Collects a fresh process sample from `sys` and posts it to `url`.
///
/// Sampling notes:
/// - `sysinfo` computes process CPU usage as a delta between refreshes.
/// - The caller keeps a persistent `System` instance and calls this function
///   in a loop, so each invocation has a previous sample to compare against.
/// - `cpu_usage` is sent as total process utilization across all cores.
async fn send_to_dashboard(sys: &mut System, url: &str) -> Result<(), Box<dyn Error>> {
    // Refresh process list + CPU stats for all known processes.
    // `with_cpu()` is required for process CPU usage updates.
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cpu(),
    );

    // Transform sampled processes into the shared payload shape.
    let processes: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p: &Process| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            cpu_usage: p.cpu_usage(),
            memory_kb: p.memory() / 1024,
            status: format!("{:?}", p.status()),
        })
        .collect();

    // Resolve local hostname once per send cycle.
    let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());

    // Use UNIX epoch seconds for easy cross-service timestamp handling.
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    // Final payload expected by the API.
    let payload = DashboardPayload {
        hostname,
        timestamp,
        processes,
    };

    // Post JSON payload to dashboard API.
    let client = reqwest::Client::new();
    let response = client.post(url).json(&payload).send().await?;

    if response.status().is_success() {
        println!(
            "✅ Sent {} processes from {} | Status: {}",
            payload.processes.len(),
            payload.hostname,
            response.status()
        );
    } else {
        eprintln!("❌ Failed | Status: {}", response.status());
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    // Ingest endpoint exposed by the API service.
    let dashboard_url = "http://localhost:3000/api/processes";

    // Keep one persistent `System` instance for stable delta-based CPU sampling.
    let mut sys = System::new_all();

    // Prime CPU sampling once so the first loop iteration has a baseline.
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cpu(),
    );

    println!("🚀 Process Sender started");
    println!("   Posting to → {}", dashboard_url);
    println!("   (Ctrl+C to stop)\n");

    // Main send loop:
    // - sample + send
    // - wait 2 seconds
    // - repeat
    loop {
        if let Err(e) = send_to_dashboard(&mut sys, dashboard_url).await {
            eprintln!("Error: {}", e);
        }
        sleep(Duration::from_secs(2)).await;
    }
}
