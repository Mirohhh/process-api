# Rust Process Dashboard

A small Rust workspace for collecting and visualizing process data from one or more hosts.

## Workspace layout

- `sender/` – collects process info and posts it to the API
- `api/` – stores host data in memory and serves:
  - REST endpoints
  - SSE live updates
  - browser dashboard (`/`)
- `shared/` – shared payload/data contract between sender and API

## Data model (shared contract)

The sender posts `DashboardPayload` objects:

- `hostname: String`
- `timestamp: u64` (Unix seconds)
- `processes: Vec<ProcessInfo>`

Each `ProcessInfo` includes:

- `pid: u32`
- `name: String`
- `cpu_usage: f32` (**total process utilization across all cores**)
- `memory_kb: u64`
- `status: String`

---

## Running locally

From workspace root:

```/dev/null/commands.sh#L1-4
cargo run -p process-dashboard-api
cargo run -p process-sender
# open http://localhost:3000
```

Sender currently posts every 2 seconds to:

- `http://localhost:3000/api/processes`

---

## API overview

### Legacy endpoints (kept for compatibility)

- `POST /api/processes`  
  Ingest a `DashboardPayload`.
- `GET /api/processes`  
  Returns latest payload among known hosts (or empty payload if none yet).

### v1 endpoints

- `GET /api/v1/hosts`  
  List known hosts and summary info.
- `GET /api/v1/hosts/{hostname}/processes`  
  Latest payload for one host.
- `GET /api/v1/hosts/{hostname}/snapshots?since=<unix>&limit=<n>`  
  Historical snapshots for one host.
- `DELETE /api/v1/hosts/{hostname}`  
  Remove host data from memory.
- `GET /api/v1/events`  
  SSE stream for typed events.

---

## Endpoint details + examples

## `POST /api/processes`

Ingests sender payload, updates in-memory state, appends snapshot history, and broadcasts a live update event.

Example:

```/dev/null/curl.sh#L1-24
curl -X POST http://localhost:3000/api/processes \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "dev-machine",
    "timestamp": 1730000000,
    "processes": [
      {
        "pid": 1234,
        "name": "chrome.exe",
        "cpu_usage": 12.4,
        "memory_kb": 512000,
        "status": "Run"
      }
    ]
  }'
```

Success response:

```/dev/null/response.json#L1-1
"✅ Data received"
```

---

## `GET /api/v1/hosts`

Returns host summaries:

- `hostname`
- `timestamp` (latest sample time)
- `process_count` (latest payload)
- `snapshot_count` (retained in memory)

Example:

```/dev/null/curl.sh#L1-1
curl http://localhost:3000/api/v1/hosts
```

Example response:

```/dev/null/response.json#L1-12
[
  {
    "hostname": "dev-machine",
    "timestamp": 1730000000,
    "process_count": 218,
    "snapshot_count": 42
  }
]
```

---

## `GET /api/v1/hosts/{hostname}/processes`

Returns latest process payload for a host.

Example:

```/dev/null/curl.sh#L1-1
curl http://localhost:3000/api/v1/hosts/dev-machine/processes
```

Not found response (`404`):

```/dev/null/response.json#L1-3
{
  "error": "Host 'unknown-host' not found"
}
```

---

## `GET /api/v1/hosts/{hostname}/snapshots?since=<unix>&limit=<n>`

Returns historical snapshots for a host.

Query params:

- `since` (optional, default `0`) — inclusive lower bound by timestamp
- `limit` (optional, default `100`) — clamped to `1..=1000`

Example:

```/dev/null/curl.sh#L1-1
curl "http://localhost:3000/api/v1/hosts/dev-machine/snapshots?since=1730000000&limit=50"
```

Not found response (`404`):

```/dev/null/response.json#L1-3
{
  "error": "Host 'unknown-host' not found"
}
```

---

## `DELETE /api/v1/hosts/{hostname}`

Removes one host and emits a `host_removed` SSE event.

Example:

```/dev/null/curl.sh#L1-1
curl -X DELETE http://localhost:3000/api/v1/hosts/dev-machine
```

Example response:

```/dev/null/response.json#L1-4
{
  "status": "ok",
  "removed": "dev-machine"
}
```

---

## `GET /api/v1/events` (SSE)

Streams typed server-sent events.

Event types:

- `host_updated`
- `host_removed`

You can test with `curl`:

```/dev/null/curl.sh#L1-1
curl -N http://localhost:3000/api/v1/events
```

Example stream output:

```/dev/null/sse.txt#L1-6
event: host_updated
data: {"type":"host_updated","hostname":"dev-machine","timestamp":1730000000,"process_count":218}

event: host_removed
data: {"type":"host_removed","hostname":"dev-machine"}
```

---

## Notes

- Storage is currently **in-memory** (data resets on API restart).
- Snapshot retention is bounded per host in API code (`MAX_SNAPSHOTS_PER_HOST`).
- Dashboard page at `/` uses:
  - host list endpoint
  - per-host processes endpoint
  - SSE updates for live refresh