// api/src/main.rs

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use dashboard_shared::DashboardPayload;
use std::sync::{Arc, Mutex};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

type AppState = Arc<Mutex<DashboardPayload>>;

#[tokio::main]
async fn main() {
    let state: AppState = Arc::new(Mutex::new(DashboardPayload {
        hostname: "".to_string(),
        timestamp: 0,
        processes: vec![],
    }));

    let app = Router::new()
        .route("/", get(dashboard_page)) // ← nice HTML page
        .route("/api/processes", get(get_all_processes)) // ← view data in browser
        .route("/api/processes", post(receive_processes)) // ← sender still works
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("🚀 Dashboard API running on http://localhost:3000");
    println!("   → Open this in your browser: http://localhost:3000");
    println!("   → Sender posts to: http://localhost:3000/api/processes\n");

    axum::serve(listener, app).await.unwrap();
}

// ─────────────────────────────────────────────────────────────
// GET /  → Beautiful dashboard page
// ─────────────────────────────────────────────────────────────
async fn dashboard_page() -> axum::response::Html<String> {
    axum::response::Html(r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Process Dashboard</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script>
        let cpuSortMode = 'none'; // none -> desc -> asc -> none

        function cpuAsPercent(value) {
            if (value === null || value === undefined || Number.isNaN(Number(value))) {
                return 0;
            }
            const numeric = Number(value);
            return numeric <= 1 ? numeric * 100 : numeric;
        }

        function formatCpu(value) {
            return cpuAsPercent(value).toFixed(1);
        }

        function toggleCpuSort() {
            if (cpuSortMode === 'none') {
                cpuSortMode = 'desc';
            } else if (cpuSortMode === 'desc') {
                cpuSortMode = 'asc';
            } else {
                cpuSortMode = 'none';
            }
            loadData();
        }

        async function loadData() {
            const res = await fetch('/api/processes');
            const data = await res.json();
            const container = document.getElementById('processes');

            if (!data || !data.processes || data.processes.length === 0) {
                container.innerHTML = `
                    <div class="text-gray-400">No process data yet.</div>
                `;
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
                        <span class="text-sm font-normal text-gray-500">${new Date(data.timestamp * 1000).toLocaleString()}</span>
                    </h2>
                    <div class="overflow-x-auto">
                        <table class="w-full text-sm border-collapse">
                            <thead>
                                <tr class="bg-gray-800 text-white">
                                    <th class="p-2 text-left">PID</th>
                                    <th class="p-2 text-left">Process</th>
                                    <th class="p-2 text-right cursor-pointer select-none" onclick="toggleCpuSort()" title="Sort by CPU (none/desc/asc)">
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
                                        <td class="p-2 text-right">${p.memory_kb.toLocaleString()}</td>
                                        <td class="p-2">${p.status}</td>
                                    </tr>
                                `).join('')}
                            </tbody>
                        </table>
                    </div>
                </div>
            `;
        }
        let pollTimer = null;

        function startPolling() {
            if (pollTimer !== null) return;
            pollTimer = setInterval(loadData, 2000);
        }

        function stopPolling() {
            if (pollTimer === null) return;
            clearInterval(pollTimer);
            pollTimer = null;
        }

        document.addEventListener('visibilitychange', () => {
            if (document.hidden) {
                stopPolling();
            } else {
                loadData();
                startPolling();
            }
        });

        window.onload = () => {
            loadData();
            startPolling();
        };
    </script>
</head>
<body class="bg-zinc-950 text-white p-8">
    <div class="max-w-7xl mx-auto">
        <h1 class="text-4xl font-bold mb-8 flex items-center gap-3">
            📡 Process Dashboard
            <span class="text-sm bg-green-500 text-black px-3 py-1 rounded-full font-mono">LIVE</span>
        </h1>
        <div id="processes" class="space-y-8"></div>
    </div>
</body>
</html>
    "#.to_string())
}

// ─────────────────────────────────────────────────────────────
// GET /api/processes → JSON (for browser or other clients)
// ─────────────────────────────────────────────────────────────
async fn get_all_processes(State(state): State<AppState>) -> Json<DashboardPayload> {
    let data = state.lock().unwrap().clone();
    Json(data)
}

// ─────────────────────────────────────────────────────────────
// POST /api/processes (unchanged)
// ─────────────────────────────────────────────────────────────
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
    *state.lock().unwrap() = payload;
    Json("✅ Data received")
}
