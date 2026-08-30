# SDKWork Audio API Server Dockerfile
# Multi-stage build for production deployment

# Stage 1: Build Rust application
FROM rust:1.75-slim as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build release
RUN cargo build --release --bin sdkwork-audio-api-server

# Stage 2: Runtime
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/sdkwork-audio-api-server /usr/local/bin/

# Copy migrations
COPY crates/sdkwork-audio-generation-repository-sqlx/migrations/ /app/migrations/

# Copy source configuration
COPY etc/ /app/etc/

# Expose port
EXPOSE 8080

# Set environment variables
ENV RUST_LOG=info
ENV DATABASE_URL=sqlite:/app/data/audio.db
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8080

# Create data directory
RUN mkdir -p /app/data

# Run the application
CMD ["sdkwork-audio-api-server"]
