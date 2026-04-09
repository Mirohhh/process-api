// api/src/main.rs

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        Html, IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use dashboard_shared::DashboardPayload;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::{RwLock, broadcast};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

/// Shared application state injected into all route handlers.
///
/// - `store` keeps host data and snapshots in memory.
/// - `events` broadcasts typed live-update events for SSE clients.
#[derive(Clone)]
struct AppState {
    store: Arc<RwLock<DashboardStore>>,
    events: broadcast::Sender<SseMessage>,
}

/// In-memory storage for host process data.
///
/// This service currently uses an in-memory store (no persistence),
/// so data is lost on restart.
#[derive(Default)]
struct DashboardStore {
    /// Per-host state keyed by hostname.
    hosts: HashMap<String, HostState>,
    /// Most recently updated host, used by the legacy endpoint.
    latest_host: Option<String>,
}

/// Data bucket for one host.
///
/// `latest` is used for quick "current state" reads while `snapshots`
/// keeps bounded historical data for trend/history APIs.
#[derive(Clone, Default)]
struct HostState {
    /// Latest payload received for this host.
    latest: Option<DashboardPayload>,
    /// Historical payloads (bounded by `MAX_SNAPSHOTS_PER_HOST`).
    snapshots: Vec<DashboardPayload>,
}

/// Lightweight host metadata returned by `GET /api/v1/hosts`.
#[derive(Serialize)]
struct HostSummary {
    /// Hostname used as stable key in the in-memory store.
    hostname: String,
    /// Timestamp (unix seconds) from the latest payload.
    timestamp: u64,
    /// Number of processes in the latest payload.
    process_count: usize,
    /// Number of snapshots retained in memory for this host.
    snapshot_count: usize,
}

/// Query parameters for the snapshots endpoint.
#[derive(Deserialize)]
struct SnapshotsQuery {
    /// Include snapshots whose timestamp is >= `since`.
    since: Option<u64>,
    /// Maximum number of snapshots to return.
    limit: Option<usize>,
}

/// Typed SSE messages emitted by `/api/v1/events`.
#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseMessage {
    /// Emitted when a host posts a new payload.
    HostUpdated {
        hostname: String,
        timestamp: u64,
        process_count: usize,
    },
    /// Emitted when a host is manually removed.
    HostRemoved { hostname: String },
}

const MAX_SNAPSHOTS_PER_HOST: usize = 300;

/// Starts the Axum HTTP server and wires all routes/state.
///
/// Exposes:
/// - Legacy ingest/read endpoint (`/api/processes`)
/// - Versioned host/snapshot endpoints (`/api/v1/...`)
/// - SSE stream (`/api/v1/events`)
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (events_tx, _events_rx) = broadcast::channel::<SseMessage>(512);

    let state = AppState {
        store: Arc::new(RwLock::new(DashboardStore::default())),
        events: events_tx,
    };

    let app = Router::new()
        .route("/", get(dashboard_page))
        .route(
            "/health",
            get(|| async { (StatusCode::OK, "OK").into_response() }),
        )
        // Legacy endpoints (kept for compatibility)
        .route("/api/processes", get(get_latest_processes))
        .route("/api/processes", post(receive_processes))
        // v1 endpoints
        .route("/api/v1/hosts", get(list_hosts))
        .route(
            "/api/v1/hosts/{hostname}/processes",
            get(get_host_processes),
        )
        .route(
            "/api/v1/hosts/{hostname}/snapshots",
            get(get_host_snapshots),
        )
        .route(
            "/api/v1/hosts/{hostname}",
            axum::routing::delete(remove_host),
        )
        .route("/api/v1/events", get(events_stream))
        .fallback(not_found)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    println!("🚀 Dashboard API running on http://localhost:3000");
    println!("   → Open this in your browser: http://localhost:3000");
    println!("   → Sender posts to: http://localhost:3000/api/processes");
    println!("   → v1 hosts: http://localhost:3000/api/v1/hosts");
    println!(
        "   → v1 snapshots: http://localhost:3000/api/v1/hosts/{{hostname}}/snapshots?since=0&limit=50"
    );
    println!("   → v1 events (SSE): http://localhost:3000/api/v1/events\n");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Builds an empty payload used by legacy compatibility responses.
fn empty_payload() -> DashboardPayload {
    DashboardPayload {
        hostname: "".to_string(),
        timestamp: 0,
        processes: vec![],
    }
}

/// Returns a JSON `404 Not Found` response with a standard error shape.
fn not_found_json(message: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

/// Catch-all 404 handler for unhandled routes.
async fn not_found() -> impl axum::response::IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Html(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>404 Not Found</title>
    <style>
        body { background: #09090b; color: #fafafa; font-family: sans-serif; display: flex; justify-content: center; align-items: center; min-height: 100vh; margin: 0; text-align: center; }
        h1 { font-size: 6rem; margin: 0; }
        p { font-size: 1.25rem; color: #a1a1aa; margin: 0.5rem 0 2rem; }
        a { background: #22c55e; color: #000; padding: 0.6rem 1.5rem; border-radius: 0.5rem; text-decoration: none; font-weight: 600; }
        a:hover { background: #16a34a; }
    </style>
</head>
<body>
    <div>
        <h1>404</h1>
        <p>This page doesn't exist</p>
        <a href="/">Go Home</a>
    </div>
</body>
</html>"#.to_string(),
        ),
    )
}

// ─────────────────────────────────────────────────────────────
// GET /  → Dashboard page (SSE-driven, multi-host, CPU sortable)
// ─────────────────────────────────────────────────────────────
/// Serves the dashboard HTML page.
///
/// The page uses:
/// - host listing from `/api/v1/hosts`
/// - host detail from `/api/v1/hosts/{hostname}/processes`
/// - live updates via SSE from `/api/v1/events`
async fn dashboard_page() -> Html<String> {
    Html(
        r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Process Dashboard</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script>
        let cpuSortMode = 'none'; // none -> desc -> asc -> none
        let selectedHost = '';
        let evtSource = null;

        function cpuAsPercent(value) {
            if (value === null || value === undefined || Number.isNaN(Number(value))) {
                return 0;
            }
            return Number(value); // total process utilization across all cores
        }

        function formatCpu(value) {
            return cpuAsPercent(value).toFixed(1);
        }

        function toggleCpuSort() {
            if (cpuSortMode === 'none') cpuSortMode = 'desc';
            else if (cpuSortMode === 'desc') cpuSortMode = 'asc';
            else cpuSortMode = 'none';
            loadHostData();
        }

        async function fetchJson(url) {
            const res = await fetch(url);
            if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
            return await res.json();
        }

        function renderNoData(message = 'No process data yet.') {
            const container = document.getElementById('processes');
            container.innerHTML = `<div class="text-gray-400">${message}</div>`;
        }

        function renderTable(data) {
            const container = document.getElementById('processes');

            if (!data || !data.processes || data.processes.length === 0) {
                renderNoData('No process data yet.');
                return;
            }

            const displayedProcesses = [...data.processes];
            if (cpuSortMode !== 'none') {
                displayedProcesses.sort((a, b) => {
                    const av = cpuAsPercent(a.cpu_usage);
                    const bv = cpuAsPercent(b.cpu_usage);
                    return cpuSortMode === 'asc' ? av - bv : bv - av;
                });
            }

            const cpuSortIcon = cpuSortMode === 'asc' ? '▲' : (cpuSortMode === 'desc' ? '▼' : '↕');

            container.innerHTML = `
                <div class="mb-8">
                    <h2 class="text-xl font-bold mb-2">${data.hostname}
                        <span class="text-sm font-normal text-gray-500">
                            ${new Date(data.timestamp * 1000).toLocaleString()}
                        </span>
                    </h2>
                    <div class="overflow-x-auto">
                        <table class="w-full text-sm border-collapse">
                            <thead>
                                <tr class="bg-gray-800 text-white">
                                    <th class="p-2 text-left">PID</th>
                                    <th class="p-2 text-left">Process</th>
                                    <th class="p-2 text-right cursor-pointer select-none"
                                        onclick="toggleCpuSort()"
                                        title="Sort by CPU (none/desc/asc)">
                                        CPU % ${cpuSortIcon}
                                    </th>
                                    <th class="p-2 text-right">Memory (KB)</th>
                                    <th class="p-2 text-left">Status</th>
                                </tr>
                            </thead>
                            <tbody>
                                ${displayedProcesses.map(p => `
                                    <tr class="border-t hover:bg-gray-50 hover:text-gray-900">
                                        <td class="p-2 font-mono">${p.pid}</td>
                                        <td class="p-2">${p.name}</td>
                                        <td class="p-2 text-right">${formatCpu(p.cpu_usage)}</td>
                                        <td class="p-2 text-right">${Number(p.memory_kb).toLocaleString()}</td>
                                        <td class="p-2">${p.status}</td>
                                    </tr>
                                `).join('')}
                            </tbody>
                        </table>
                    </div>
                </div>
            `;
        }

        async function refreshHosts(preferredHost = '') {
            const hosts = await fetchJson('/api/v1/hosts');
            const select = document.getElementById('hostSelect');
            const current = selectedHost;

            if (!hosts || hosts.length === 0) {
                selectedHost = '';
                select.innerHTML = '<option value="">No hosts</option>';
                renderNoData('No hosts reporting yet.');
                return;
            }

            select.innerHTML = hosts.map(h =>
                `<option value="${h.hostname}">${h.hostname} (${h.process_count} procs)</option>`
            ).join('');

            if (current && hosts.some(h => h.hostname === current)) {
                selectedHost = current;
            } else if (preferredHost && hosts.some(h => h.hostname === preferredHost)) {
                selectedHost = preferredHost;
            } else {
                selectedHost = hosts[0].hostname;
            }

            select.value = selectedHost;
            await loadHostData();
        }

        async function loadHostData() {
            if (!selectedHost) {
                renderNoData('No host selected.');
                return;
            }

            try {
                const hostPath = encodeURIComponent(selectedHost);
                const data = await fetchJson(`/api/v1/hosts/${hostPath}/processes`);
                renderTable(data);
            } catch (_e) {
                renderNoData('Host not found or failed to load data.');
            }
        }

        async function refreshSelectedHostData() {
            if (!selectedHost) return;
            await loadHostData();
        }

        function startEvents() {
            if (evtSource !== null) return;

            evtSource = new EventSource('/api/v1/events');

            evtSource.addEventListener('host_updated', async (event) => {
                try {
                    const msg = JSON.parse(event.data);
                    await refreshHosts('');
                    await refreshSelectedHostData();
                } catch (_e) {
                    await refreshHosts('');
                }
            });

            evtSource.addEventListener('host_removed', async (event) => {
                try {
                    const msg = JSON.parse(event.data);
                    if (msg.hostname === selectedHost) {
                        selectedHost = '';
                    }
                    await refreshHosts('');
                } catch (_e) {
                    await refreshHosts('');
                }
            });

            evtSource.onerror = () => {
                // Browser auto-reconnects SSE by default.
            };
        }

        function stopEvents() {
            if (evtSource !== null) {
                evtSource.close();
                evtSource = null;
            }
        }

        document.addEventListener('visibilitychange', async () => {
            if (document.hidden) {
                stopEvents();
            } else {
                startEvents();
                await refreshHosts(selectedHost);
            }
        });

        window.onload = async () => {
            const select = document.getElementById('hostSelect');
            select.addEventListener('change', async (e) => {
                selectedHost = e.target.value;
                await loadHostData();
            });

            await refreshHosts('');
            startEvents();
        };
    </script>
</head>
<body class="bg-zinc-950 text-white p-8">
    <div class="max-w-7xl mx-auto">
        <div class="flex items-center justify-between mb-8 gap-4 flex-wrap">
            <h1 class="text-4xl font-bold flex items-center gap-3">
                Process Dashboard
                <span class="text-sm bg-green-500 text-black px-3 py-1 rounded-full font-mono">LIVE (SSE)</span>
            </h1>

            <div class="flex items-center gap-2">
                <label for="hostSelect" class="text-sm text-gray-300">Host:</label>
                <select id="hostSelect" class="bg-gray-800 text-white border border-gray-700 rounded px-3 py-2"></select>
            </div>
        </div>

        <div id="processes" class="space-y-8"></div>
    </div>
</body>
</html>
"#.to_string(),
    )
}

// ─────────────────────────────────────────────────────────────
// GET /api/processes (legacy): latest host payload
// ─────────────────────────────────────────────────────────────
/// Legacy read endpoint returning the latest payload among all hosts.
///
/// This is kept for backward compatibility with earlier dashboard code.
async fn get_latest_processes(State(state): State<AppState>) -> Json<DashboardPayload> {
    let store = state.store.read().await;
    if let Some(latest) = &store.latest_host {
        if let Some(host) = store.hosts.get(latest) {
            if let Some(payload) = &host.latest {
                return Json(payload.clone());
            }
        }
    }
    Json(empty_payload())
}

// ─────────────────────────────────────────────────────────────
// GET /api/v1/hosts
// ─────────────────────────────────────────────────────────────
/// Lists all currently known hosts with summary metadata.
async fn list_hosts(State(state): State<AppState>) -> Json<Vec<HostSummary>> {
    let store = state.store.read().await;

    let mut hosts: Vec<HostSummary> = store
        .hosts
        .iter()
        .filter_map(|(hostname, host)| {
            host.latest.as_ref().map(|p| HostSummary {
                hostname: hostname.clone(),
                timestamp: p.timestamp,
                process_count: p.processes.len(),
                snapshot_count: host.snapshots.len(),
            })
        })
        .collect();

    hosts.sort_by(|a, b| a.hostname.cmp(&b.hostname));
    Json(hosts)
}

// ─────────────────────────────────────────────────────────────
// GET /api/v1/hosts/{hostname}/processes
// Returns 404 for unknown hosts
// ─────────────────────────────────────────────────────────────
/// Returns the latest payload for a specific host.
///
/// Responds with JSON `404` when the host does not exist.
async fn get_host_processes(
    Path(hostname): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let store = state.store.read().await;
    match store.hosts.get(&hostname).and_then(|h| h.latest.clone()) {
        Some(payload) => Json(payload).into_response(),
        None => not_found_json(&format!("Host '{}' not found", hostname)),
    }
}

// ─────────────────────────────────────────────────────────────
// GET /api/v1/hosts/{hostname}/snapshots?since=<unix>&limit=<n>
// Returns 404 for unknown hosts
// ─────────────────────────────────────────────────────────────
/// Returns historical snapshots for a host, filtered by `since` and `limit`.
///
/// Query params:
/// - `since`: unix timestamp lower bound (inclusive), default `0`
/// - `limit`: max returned rows, default `100`, clamped to `1..=1000`
///
/// Responds with JSON `404` when the host does not exist.
async fn get_host_snapshots(
    Path(hostname): Path<String>,
    Query(params): Query<SnapshotsQuery>,
    State(state): State<AppState>,
) -> Response {
    let store = state.store.read().await;
    let host = match store.hosts.get(&hostname) {
        Some(h) => h,
        None => return not_found_json(&format!("Host '{}' not found", hostname)),
    };

    let since = params.since.unwrap_or(0);
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);

    let mut snapshots: Vec<DashboardPayload> = host
        .snapshots
        .iter()
        .filter(|p| p.timestamp >= since)
        .cloned()
        .collect();

    if snapshots.len() > limit {
        snapshots = snapshots[snapshots.len() - limit..].to_vec();
    }

    Json(snapshots).into_response()
}

// ─────────────────────────────────────────────────────────────
// DELETE /api/v1/hosts/{hostname}
// Removes host + emits host_removed event
// ─────────────────────────────────────────────────────────────
/// Deletes a host from memory and emits a `host_removed` SSE event.
///
/// Also updates the legacy `latest_host` pointer if needed.
/// Responds with JSON `404` when the host does not exist.
async fn remove_host(Path(hostname): Path<String>, State(state): State<AppState>) -> Response {
    let removed = {
        let mut store = state.store.write().await;
        let existed = store.hosts.remove(&hostname).is_some();

        if existed && store.latest_host.as_deref() == Some(hostname.as_str()) {
            store.latest_host = store.hosts.keys().next().cloned();
        }

        existed
    };

    if !removed {
        return not_found_json(&format!("Host '{}' not found", hostname));
    }

    let _ = state.events.send(SseMessage::HostRemoved {
        hostname: hostname.clone(),
    });

    Json(serde_json::json!({ "status": "ok", "removed": hostname })).into_response()
}

// ─────────────────────────────────────────────────────────────
// GET /api/v1/events  (typed SSE stream)
// ─────────────────────────────────────────────────────────────
/// Streams typed server-sent events for host lifecycle updates.
///
/// Event types:
/// - `host_updated`
/// - `host_removed`
async fn events_stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.events.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(event_msg) => {
            let event_name = match event_msg {
                SseMessage::HostUpdated { .. } => "host_updated",
                SseMessage::HostRemoved { .. } => "host_removed",
            };

            let payload = serde_json::to_string(&event_msg).ok()?;
            Some(Ok(Event::default().event(event_name).data(payload)))
        }
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

// ─────────────────────────────────────────────────────────────
// POST /api/processes  (ingest sender payload)
// ─────────────────────────────────────────────────────────────
/// Ingests a sender payload, updates in-memory state, and emits live events.
///
/// Side effects:
/// - updates per-host `latest` payload
/// - appends to per-host snapshots with bounded retention
/// - sets `latest_host` for legacy reads
/// - broadcasts `host_updated` over SSE
async fn receive_processes(
    State(state): State<AppState>,
    Json(payload): Json<DashboardPayload>,
) -> Json<&'static str> {
    println!(
        "📥 Received {} processes from {} at {}",
        payload.processes.len(),
        payload.hostname,
        payload.timestamp
    );

    let hostname = payload.hostname.clone();
    let timestamp = payload.timestamp;
    let process_count = payload.processes.len();

    {
        let mut store = state.store.write().await;
        let host = store
            .hosts
            .entry(hostname.clone())
            .or_insert_with(HostState::default);

        host.latest = Some(payload.clone());
        host.snapshots.push(payload);

        if host.snapshots.len() > MAX_SNAPSHOTS_PER_HOST {
            let extra = host.snapshots.len() - MAX_SNAPSHOTS_PER_HOST;
            host.snapshots.drain(0..extra);
        }

        store.latest_host = Some(hostname.clone());
    }

    let _ = state.events.send(SseMessage::HostUpdated {
        hostname,
        timestamp,
        process_count,
    });

    Json("✅ Data received")
}
