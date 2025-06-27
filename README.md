A secure, privacy-preserving wallet infrastructure powered by:
- **Actix Web** (Rust backend)
- **AWS Nitro Enclaves** (Trusted Execution Environment)
- **Terraform** (Infrastructure as Code for AWS provisioning)

## 1. Backend (Actix Web)
  Receives incoming wallet calls
  Communicates with TEE (AWS Nitro) using vsock
  Returns response to client(s)

## AWS Nitro Enclaves (Rust)
  All cryptography is here
  Wallet creation
  Sign ETH
  Sign SOL

## Terraform
  IaC to deploy this repo into EC2
  Create Nitro eif package
  Deploy eif package inside EC2
  VPC
  SG
  other infra stuff
  read main.tf and .tpl file for all the steps

## Infra (EC2)
  AWS Nitro enclave (Trusted Execution Environment)
  Actix backend deployed and listening @ 8080
  
