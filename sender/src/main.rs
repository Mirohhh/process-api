// sender/src/main.rs  (CLEANED — no unused imports)

use dashboard_shared::{DashboardPayload, ProcessInfo};
use std::error::Error;
use sysinfo::{Process, ProcessRefreshKind, ProcessesToUpdate, System}; // ← Pid removed
use tokio::time::{sleep, Duration};

async fn send_to_dashboard(sys: &mut System, url: &str) -> Result<(), Box<dyn Error>> {
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cpu(),
    );

    let cores = sys.cpus().len().max(1) as f32;

    let processes: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p: &Process| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            cpu_usage: p.cpu_usage() / cores,
            memory_kb: p.memory() / 1024,
            status: format!("{:?}", p.status()),
        })
        .collect();

    let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let payload = DashboardPayload {
        hostname,
        timestamp,
        processes,
    };

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
    let dashboard_url = "http://localhost:3000/api/processes";
    let mut sys = System::new_all();

    // Prime CPU sampling once so the first real loop has a previous sample.
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cpu(),
    );

    println!("🚀 Process Sender started");
    println!("   Posting to → {}", dashboard_url);
    println!("   (Ctrl+C to stop)\n");

    loop {
        if let Err(e) = send_to_dashboard(&mut sys, dashboard_url).await {
            eprintln!("Error: {}", e);
        }
        sleep(Duration::from_secs(2)).await;
    }
}
