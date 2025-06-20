terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 4.16"
    }
    tls = {
      source  = "hashicorp/tls"
      version = "~> 4.16"
    }
    local = {
      source  = "hashicorp/local"
      version = "~> 4.16"
    }
    required_version = ">= 1.2.0"
  }
}

provider "aws" {
  region = var.region
}

variable "region" {
  type    = string
  default = "us-east-1"
}

data "aws_availability_zones" "azs" {}

data "aws_ami" "amazon_linux2_arm64" {
  most_recent = true
  owners      = ["amazon"]

  filter {
    name   = "name"
    values = ["amzn2-ami-hvm-*-gp2"]
  }
  filter {
    name   = "architecture"
    values = ["arm64"]
  }
}

# 1/ Generate a local ssh keypair
resource "tls_private_key" "ssh" {
  algorithm = "RSA"
  rsa_bits  = 4096
}

resource "local_file" "private_key" {
  content         = tls_private_key.ssh.private_key_pem
  filename        = "${path.module}/enclave_key.pem"
  file_permission = "0600"
}

resource "aws_key_pair" "deployer" {
  key_name   = "enclave_key"
  public_key = tls_private_key.ssh.public_key_openssh
}

# 2/ Networking, VPC, subnet, IGW, route table
resource "aws_vpc" "main" {
  cidr_block           = "10.0.0.0/16"
  enable_dns_support   = true
  enable_dns_hostnames = true
  tags                 = { Name = "enclave_vpc" }
}

resource "aws_internet_gateway" "igw" {
  vpc_id = aws_vpc.main.id
  tags   = { Name = "enclave_igw" }
}

resource "aws_subnet" "public" {
  vpc_id                  = aws_vpc.main.id
  cidr_block              = "10.0.1.0/24"
  availability_zone       = data.aws_availability_zones.azs.names[0]
  map_public_ip_on_launch = true
  tags                    = { Name = "enclave_public_subnet" }
}

resource "aws_route_table" "public" {
  vpc_id = aws_vpc.main.id

  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.igw.id
  }
  tags = { Name = "enclave_public_rt" }
}

resource "aws_route_table_association" "public_assoc" {
  subnet_id      = aws_subnet.public.id
  route_table_id = aws_route_table.public.id
}

# 3/ Security group
resource "aws_security_group" "enclave_sg" {
  name        = "enclave_sg"
  description = "Allow SSH and API"
  vpc_id      = aws_vpc.main.id

  ingress {
    description = "SSH"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  ingress {
    description = "API"
    from_port   = 8080
    to_port     = 8080
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = { Name = "enclave_sg" }
}

# 4/ Nitro enabled EC2 with user_data
resource "aws_instance" "enclave" {
  ami                         = data.aws_ami.amazon_linux2_arm64
  instance_type               = "t4g.nano"
  key_name                    = aws_key_pair.deployer.key_name
  subnet_id                   = aws_subnet.public.id
  vpc_security_group_ids      = [aws_security_group.enclave_sg.id]
  associate_public_ip_address = true

  # enable nitro enclaves
  enclave_options {
    enabled = true
  }

  # install & run everything
  user_data = <<-EOF
    #!/bin/bash
    yum update -y
    yum install -y docker
    systemctl enable --now docker

    amazon-linux-extras enable aws-nitro-enclaves-cli
    yum install -y aws-nitro-enclaves-cli

    cat > /etc/nitro_enclaves/allocator.yaml << 'EOM'
    ---
    memory_mib: 1024
    cpu_count: 2
    EOM
    systemctl enable --now nitro-enclaves-allocator.service

    curl https://sh.rustup.rs -sSf | sh -s -- -y
    export PATH=/root/.cargo/bin:$PATH

    cd /root
    git clone https://github.com/rajakath80/nitro.git workspace

    # Build & package enclave
    cd workspace/nitro
    docker build -f Dockerfile -t enclave-builder .
    docker run --rm --privileged --device /dev/kvm \\
        -v $(pwd) :/workspace -w /workspace enclave-builer \\
        nitro-cli build-enclave \\
            --binary-path target/release/enclave-wallet \\
            --output-file wallet_enclave.eif
    nitro-cli run-enclave --eif-path wallet_enclave.eif \\
        --cpu-count 2 --memory 1024 --enclave-cid 3 &

    # Build & run API backend
    cd ../backend
    cargo build --release
    nohup target/release/backend &
    EOF

  tags = { Name = "enclave_instance" }
}

# 5/ Expose the public IP
output "public_ip" {
  description = "Publid IP of the nitro ec2 instance"
  value       = aws_instance.enclave.public_ip
}
