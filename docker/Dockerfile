# Multi-stage build for production optimization
# Stage 1: Build stage - Compile Rust binary
FROM rust:1.83-slim AS builder

WORKDIR /app

# Install system dependencies needed for building
RUN apt-get update && apt-get install -y \
    build-essential \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    libprotobuf-dev \
    clang \
    libclang-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy Cargo files for dependency caching
COPY Cargo.toml Cargo.lock ./

# Copy workspace member Cargo.toml files
COPY krc721d/Cargo.toml ./krc721d/
COPY krc721d/build.rs ./krc721d/
COPY client/Cargo.toml ./client/
COPY cluster/Cargo.toml ./cluster/
COPY core/Cargo.toml ./core/
COPY database/Cargo.toml ./database/
COPY http/server/Cargo.toml ./http/server/
COPY kaspad/Cargo.toml ./kaspad/
COPY macros/Cargo.toml ./macros/
COPY nexus/Cargo.toml ./nexus/
COPY rpc/client/Cargo.toml ./rpc/client/
COPY rpc/core/Cargo.toml ./rpc/core/
COPY rpc/server/Cargo.toml ./rpc/server/
COPY snapshot/Cargo.toml ./snapshot/
COPY wasm/Cargo.toml ./wasm/
COPY testing/Cargo.toml ./testing/

# Copy .git directory for vergen (build.rs needs git info)
# This is needed for version information in the binary
COPY .git ./.git

# Copy all source code
COPY . .

# Build the release binary
# Using --workspace to ensure all dependencies are built
RUN cargo build --release --bin krc721d

# Stage 2: Runtime stage - Minimal runtime image
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder stage
COPY --from=builder /app/target/release/krc721d /usr/local/bin/krc721d

# Copy configuration file if it exists
COPY --from=builder /app/krc721d.toml /app/krc721d.toml

# Create data directory
RUN mkdir -p /app/data

# Expose default port (adjust if needed)
EXPOSE 8080

# Run the binary
CMD ["/usr/local/bin/krc721d"]

