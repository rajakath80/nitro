#!/bin/bash
# Ensure HOME is defined before ‘set -u’
export HOME=/root

set -euo pipefail

# 1) Base setup: Docker, AWS CLI, Git
yum update -y
yum install -y aws-cli git docker gcc gcc-c++ make
systemctl enable --now docker

# 2) Nitro CLI + kernel modules
amazon-linux-extras enable aws-nitro-enclaves-cli
yum install -y aws-nitro-enclaves-cli aws-nitro-enclaves-cli-devel
modprobe nitro_enclaves
modprobe vsock

# 3) Allocator config (reserve 1 vCPU for enclave)
cat > /etc/nitro_enclaves/allocator.yaml << 'EOM'
---
memory_mib: 1024
cpu_count: 1
EOM
systemctl enable --now nitro-enclaves-allocator.service

# 4) Install Rust toolchain
curl https://sh.rustup.rs -sSf | sh -s -- -y

# 5) Source Cargo’s Bash env
. "$HOME/.cargo/env"

# 6) AWS CLI region
export AWS_DEFAULT_REGION=${aws_region}

# 7) Fetch GitHub PAT from SSM
TOKEN=$(aws ssm get-parameter --name /github/pat --with-decryption --query Parameter.Value --output text)

# 8) Clone & build enclave
cd /root
git clone https://$${TOKEN}@github.com/rajakath80/nitro.git workspace
cd workspace/nitro

# 9) Build the enclave docker image
docker build -f Dockerfile -t nitro:latest .

# 10) Package it into an EIF (no Docker container needed)
nitro-cli build-enclave --docker-uri nitro:latest --output-file wallet_enclave.eif

# 11) Launch the enclave (detached)
nohup nitro-cli run-enclave --eif-path wallet_enclave.eif --cpu-count 1 --memory 1024 --enclave-cid 3 > /var/log/enclave.log 2>&1 &

# 12) Build & run Actix-Web backend
cd ../backend
cargo build --release
nohup target/release/backend > /var/log/backend.log 2>&1 &