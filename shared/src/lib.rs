use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32, // percent
    pub memory_kb: u64,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DashboardPayload {
    pub hostname: String,
    pub timestamp: u64, // Unix timestamp in seconds
    pub processes: Vec<ProcessInfo>,
}
