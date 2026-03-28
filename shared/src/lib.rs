//! Shared data contracts for the process dashboard workspace.
//!
//! This crate defines the payload shapes exchanged between the sender and API.

use serde::{Deserialize, Serialize};

/// Snapshot of a single OS process reported by a sender.
///
/// Each [`ProcessInfo`] entry represents one process at the moment the sender
/// sampled system state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProcessInfo {
    /// Operating system process ID.
    pub pid: u32,
    /// Human-readable process name.
    pub name: String,
    /// CPU utilization percentage for this process.
    ///
    /// This value is expressed as total process utilization across all cores.
    pub cpu_usage: f32,
    /// Memory usage in kibibytes (KiB).
    pub memory_kb: u64,
    /// Process status as a stringified platform-specific state.
    pub status: String,
}

/// Top-level payload sent by a sender and consumed by the API/dashboard.
///
/// A payload contains host metadata plus a list of sampled processes.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DashboardPayload {
    /// Hostname of the machine that produced this payload.
    pub hostname: String,
    /// Unix timestamp (seconds) when the sample was created.
    pub timestamp: u64,
    /// Process list captured at [`Self::timestamp`].
    pub processes: Vec<ProcessInfo>,
}
