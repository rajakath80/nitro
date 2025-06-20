# nitro/Dockerfile

### Stage 1: build the enclave binary ###
FROM rust:1.87.0 AS builder

# 1. Set working dir and copy your enclave code
WORKDIR /usr/src/nitro
COPY . .

# 2. Compile in release mode
RUN cargo build --release

# DEBUG: inspect what got built
RUN ls -l target/release

### Stage 2: produce the minimal enclave image ###
FROM amazonlinux:2

# install glibc & SSL libs needed by your binary
RUN yum install -y openssl-libs ca-certificates && yum clean all

# 3. Copy the compiled binary into the enclave image
COPY --from=builder /usr/src/nitro/target/release/nitro /enclave/nitro

# 4. At runtime, this is the enclave entrypoint
ENTRYPOINT ["/enclave/nitro"]