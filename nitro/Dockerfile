# nitro/Dockerfile

### Stage 1: build the enclave binary ###
FROM rust:1.72 as builder

# 1. Set working dir and copy your enclave code
WORKDIR /usr/src/enclave-wallet
COPY . .

# 2. Compile in release mode
RUN cargo build --release

### Stage 2: produce the minimal enclave image ###
FROM scratch

# 3. Copy the compiled binary into the enclave image
COPY --from=builder /usr/src/enclave-wallet/target/release/enclave-wallet /enclave/enclave-wallet

# 4. At runtime, this is the enclave entrypoint
ENTRYPOINT ["/enclave/enclave-wallet"]