terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 4.16"
    }
    tls = {
      source  = "hashicorp/tls"
      version = "~> 4.1.0"
    }
    local = {
      source  = "hashicorp/local"
      version = "~> 2.2.0"
    }
  }
  required_version = ">= 1.2.0"
}

provider "aws" {
  region = var.region
}

variable "region" {
  type    = string
  default = "us-east-1"
}

data "aws_caller_identity" "current" {}

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

# 3/ IAM role to access SSM secrets (github pat)
resource "aws_iam_role" "instance_role" {
  name               = "enclave_instance_role"
  assume_role_policy = data.aws_iam_policy_document.ec2_assume.json
}

data "aws_iam_policy_document" "ec2_assume" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["ec2.amazonaws.com"]
    }
  }
}

# inline policy allowing SSM:GetParameter
resource "aws_iam_role_policy" "allow_ssm" {
  name = "AssumeGetGithubPAT"
  role = aws_iam_role.instance_role.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action   = ["ssm:GetParameter"]
      Resource = "arn:aws:ssm:${var.region}:${data.aws_caller_identity.current.account_id}:parameter/github/pat"
      Effect   = "Allow"
    }]
  })
}

# Attach the role to our instance via an instance profile
resource "aws_iam_instance_profile" "instance_profile" {
  name = "enclave_instance_profile"
  role = aws_iam_role.instance_role.name
}

# 4/ Nitro enabled EC2 with user_data
resource "aws_instance" "enclave" {
  ami                         = data.aws_ami.amazon_linux2_arm64.id
  instance_type               = "c6g.large"
  key_name                    = aws_key_pair.deployer.key_name
  subnet_id                   = aws_subnet.public.id
  vpc_security_group_ids      = [aws_security_group.enclave_sg.id]
  associate_public_ip_address = true

  # enable nitro enclaves
  enclave_options { enabled = true }

  iam_instance_profile = aws_iam_instance_profile.instance_profile.name

  # install & run everything
  user_data = templatefile("${path.module}/user_data_sh.tpl", {
    aws_region = var.region
  })

  tags = { Name = "enclave_instance" }
}

# 5/ Expose the public IP
output "public_ip" {
  description = "Public IP of the nitro ec2 instance"
  value       = aws_instance.enclave.public_ip
}
