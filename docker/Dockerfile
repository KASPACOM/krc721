FROM ubuntu:24.04 AS runtime

WORKDIR /app

# Install runtime dependencies
# Rust binaries typically need: libc, libssl, libgcc, and other system libraries
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libc6 \
    libgcc-s1 \
    libstdc++6 \
    zlib1g \
    file \
    && rm -rf /var/lib/apt/lists/*

# Copy the pre-built binary from CI build
# The binary is built in GitHub Actions and downloaded as artifact
COPY target/release/krc721d /usr/local/bin/krc721d

# Copy configuration file to a location the binary can find
# The binary searches from executable location upward: /usr/local/bin -> /usr/local -> /usr -> /
# So we'll place it in /usr/local/ which is the first parent directory
COPY krc721d.toml /usr/local/krc721d.toml

# Verify binary and check dependencies
RUN chmod +x /usr/local/bin/krc721d && \
    echo "=== Checking binary type ===" && \
    file /usr/local/bin/krc721d && \
    echo "=== Checking binary dependencies ===" && \
    ldd /usr/local/bin/krc721d 2>&1 | head -20 || echo "Note: Binary may be statically linked or ldd unavailable" && \
    echo "=== Testing binary with --help ===" && \
    /usr/local/bin/krc721d --help 2>&1 | head -10 || echo "Binary help command completed" && \
    echo "=== Verifying config file location ===" && \
    test -f /usr/local/krc721d.toml && echo "Config file found at /usr/local/krc721d.toml" || echo "Warning: Config file not found"

# Create data directory (binary will also create ~/.krc721 by default)
RUN mkdir -p /app/data

# Expose default port (adjust if needed)
EXPOSE 8080

# Run the binary
# Users can pass arguments: docker run <image> --mainnet --sync=https://...
ENTRYPOINT ["/usr/local/bin/krc721d"]
CMD []