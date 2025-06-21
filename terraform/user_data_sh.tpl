#!/bin/bash
# Ensure HOME is defined before ‘set -u’
export HOME=/root

# Exit on errors, undefined vars, or any step in a pipeline failing;
# and print each command before executing it.
set -euxo pipefail

echo "=== Step 1: Base setup: Docker, AWS CLI, Git ==="
yum update -y
yum install -y aws-cli git docker gcc gcc-c++ make socat
systemctl enable --now docker

echo "=== Step 2: Nitro CLI + kernel modules ==="
amazon-linux-extras enable aws-nitro-enclaves-cli
yum install -y aws-nitro-enclaves-cli aws-nitro-enclaves-cli-devel
modprobe nitro_enclaves
modprobe vsock

echo "=== Step 3: Allocator config (reserve 1 vCPU for enclave) ==="
cat > /etc/nitro_enclaves/allocator.yaml << 'EOM'
---
memory_mib: 1024
cpu_count: 1
EOM
systemctl enable --now nitro-enclaves-allocator.service
 
echo "=== Step 4: Install Rust toolchain ==="
curl https://sh.rustup.rs -sSf | sh -s -- -y

echo "=== Step 5: Source Cargos Bash env ==="
. "$HOME/.cargo/env"

echo "=== Step 6: set AWS CLI region ==="
export AWS_DEFAULT_REGION=${aws_region}

echo "=== Step 7: Fetch Github PAT from SSM ==="
TOKEN=$(aws ssm get-parameter --name /github/pat --with-decryption --query Parameter.Value --output text)

echo "=== Step 8: Clone and build the enclave ==="
cd /root
git clone https://$${TOKEN}@github.com/rajakath80/nitro.git workspace
cd workspace/nitro

echo "=== Step 9: Building the enclave Docker image ==="
docker build -f Dockerfile -t nitro:latest .

echo "=== Step 10: Packaging into an EIF ==="
nitro-cli build-enclave --docker-uri nitro:latest --output-file wallet_enclave.eif

echo "=== Step 11: Launch the enclave (detached) ==="
nohup nitro-cli run-enclave --eif-path wallet_enclave.eif --cpu-count 1 --memory 1024 --enclave-cid 19 > /var/log/enclave.log 2>&1 &

echo "=== Step 12: Start logging enclave console ==="
ENCLAVE_ID=$(nitro-cli describe-enclaves | jq -r '.[0].EnclaveID')

nohup nitro-cli console --enclave-id $ENCLAVE_ID > /var/log/enclave-console.log 2>&1 &

echo "Waiting 3s for the enclave to come up…"
sleep 3

echo "=== Step 13: Start socat proxy (TCP 8080 → VSOCK 19:1024) ==="
nohup socat TCP-LISTEN:8080,reuseaddr,fork VSOCK-CONNECT:19:1024 > /var/log/socat.log 2>&1 &

# running this locally
# echo "=== Step 13: Build & run Actix-Web backend==="
# cd ../backend
# cargo build --release
# nohup target/release/backend > /var/log/backend.log 2>&1 &