# Stage 1: build the Rust binary
FROM rust:1.87.0 AS builder

WORKDIR /usr/src/backend

# Copy your Cargo manifests and source code
COPY Cargo.toml Cargo.lock ./

# Build just dependencies to cache them
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Now copy in your real source
COPY src ./src
RUN cargo build --release

# Stage 2: create a minimal runtime image
FROM debian:bookworm-slim

# Install any runtime deps (e.g. TLS, system libs)
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates \
      libssl3 \
 && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary
COPY --from=builder /usr/src/backend/target/release/backend /usr/local/bin/backend

# Expose your Actix-Web port
EXPOSE 8080

# Run the server
ENTRYPOINT ["/usr/local/bin/backend"]
