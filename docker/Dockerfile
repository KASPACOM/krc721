FROM ubuntu:24.04 AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the pre-built binary from CI build
# The binary is built in GitHub Actions and downloaded as artifact
COPY target/release/krc721d /usr/local/bin/krc721d

# Copy configuration file
COPY krc721d.toml /app/krc721d.toml

# Create data directory
RUN mkdir -p /app/data

# Expose default port (adjust if needed)
EXPOSE 8080

# Run the binary
CMD ["/usr/local/bin/krc721d"]