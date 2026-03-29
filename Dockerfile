# ====================== Dockerfile for process-dashboard-api ======================

FROM rust:1.94.0 AS builder

# Set working directory
WORKDIR /app

# Install system dependencies (for sysinfo and other crates if needed)
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY shared/ shared/
COPY api/ api/
# We don't need sender for the API image, but we copy it anyway for workspace consistency
COPY sender/ sender/

# Build only the API in release mode
RUN cargo build --release -p process-dashboard-api

# ====================== Final minimal image ======================
FROM debian:bookworm-slim

# Install runtime dependencies (if needed)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from builder stage
COPY --from=builder /app/target/release/process-dashboard-api /app/process-dashboard-api

# Expose the port (Railway will override with $PORT)
EXPOSE 3000

# Run the binary
CMD ["./process-dashboard-api"]
