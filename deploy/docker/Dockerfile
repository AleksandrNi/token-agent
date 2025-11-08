# =====================================================
# 1. Build Stage
# =====================================================
FROM rust:1.82-slim-bullseye AS builder

WORKDIR /usr/src/token-agent

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY examples ./examples

RUN cargo build --release


# =====================================================
# 2. Runtime Stage with systemd
# =====================================================
FROM ubuntu:22.04
ENV container=docker

# Install runtime dependencies + supervisord
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl supervisor \
 && apt-get clean && rm -rf /var/lib/apt/lists/*

# Create directories
RUN mkdir -p /etc/token-agent /var/log/token-agent /etc/supervisor/conf.d

# Copy binary & default config
COPY --from=builder /usr/src/token-agent/target/release/token-agent /usr/local/bin/token-agent
COPY examples/google_metadata_token.yaml /etc/token-agent/config.yaml

# Copy supervisord config
COPY deploy/supervisord.conf /etc/supervisor/conf.d/token-agent.conf

# Expose ports
EXPOSE 8080 9100

# Start supervisord
CMD ["/usr/bin/supervisord", "-n", "-c", "/etc/supervisor/conf.d/token-agent.conf"]
