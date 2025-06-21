use std::io::Read;

use bs58;
use ed25519_dalek::{
    Signature as SolSignature, Signer as SolSigner, SigningKey as SolSigningKey,
    VerifyingKey as SolVerifyingKey,
};
use hex;
use k256::ecdsa::{
    Signature as EthSignature, SigningKey as EthSigningKey, signature::Signer as EthSigner,
};
use k256::elliptic_curve::generic_array::GenericArray;
use k256::elliptic_curve::rand_core::OsRng;
use serde::Serialize;
use tiny_keccak::{Hasher, Keccak};
use tokio::io::AsyncWriteExt;
use tokio_vsock::{VMADDR_CID_ANY, VsockAddr, VsockListener};

#[derive(Serialize, Debug)]
#[serde(tag = "coin", rename_all = "lowercase")]
enum Wallet {
    Eth {
        private_key: String,
        public_key: String,
        address: String,
    },
    Sol {
        private_key: String,
        public_key: String,
    },
}

/// Generate an Ethereum wallet (secp256k1-based) with address
fn generate_eth_wallet(rng: &mut OsRng) -> (String, String, String) {
    let sk = EthSigningKey::random(rng);
    let pk = sk.verifying_key();
    let sk_hex = hex::encode(sk.to_bytes());
    // Keep the encoded point alive while we borrow its bytes
    let encoded_pk = pk.to_encoded_point(false);
    let pk_bytes = encoded_pk.as_bytes();

    // keccak256 of uncompressed pubkey (skip 0x04 prefix)
    let mut hasher = Keccak::v256();
    hasher.update(&pk_bytes[1..]);
    let mut output = [0u8; 32];
    hasher.finalize(&mut output);
    let address = hex::encode(&output[12..]);

    (
        sk_hex.clone(),
        hex::encode(pk_bytes),
        format!("0x{}", address),
    )
}

fn bytes_to_base58_string(bytes: &[u8]) -> String {
    bs58::encode(bytes).into_string()
}

/// Generate a Solana wallet (ed25519-based)
fn generate_sol_wallet(rng: &mut OsRng) -> (String, String) {
    // ed25519_dalek::Keypair is now imported directly as Keypair
    let sk = SolSigningKey::generate(rng);
    let vk = SolVerifyingKey::from(&sk);

    // Extract bytes for secret and public keys
    let sk_bytes = sk.to_bytes(); // [u8; 32]
    let pk_bytes = vk.to_bytes(); // [u8; 32]

    // Encode to Base58 or hex as desired
    let sk_bs58 = bytes_to_base58_string(&sk_bytes);
    let pk_bs58 = bytes_to_base58_string(&pk_bytes);

    (sk_bs58, pk_bs58)
}

/// Sign a message with an Ethereum private key (hex)
fn sign_eth_message(sk_hex: &str, message: &[u8]) -> Result<String, k256::ecdsa::Error> {
    // Decode hex into Vec<u8>, then convert to [u8; 32]
    let sk_vec = hex::decode(sk_hex).expect("Invalid hex for private key");
    let sk_arr: [u8; 32] = sk_vec
        .as_slice()
        .try_into()
        .expect("private key must be 32 bytes");
    // Convert to GenericArray for SigningKey
    let ga = GenericArray::clone_from_slice(&sk_arr);
    let sk = EthSigningKey::from_bytes(&ga)?;
    let sig: EthSignature = sk.sign(message);
    Ok(hex::encode(sig.to_bytes()))
}

/// Sign a message with a Solana private key (base58)
fn sign_sol_message(sk_b58: &str, message: &[u8]) -> String {
    // Decode base58 into Vec<u8>
    let sk_vec = bs58::decode(sk_b58)
        .into_vec()
        .expect("Invalid base58 for secret key");
    // Convert into fixed-size array
    let sk_arr: [u8; 32] = sk_vec
        .as_slice()
        .try_into()
        .expect("secret key must be 32 bytes");
    // Build the SigningKey from the array reference
    let sk = SolSigningKey::from_bytes(&sk_arr);
    // Sign and hex-encode signature
    let sig: SolSignature = sk.sign(message);
    hex::encode(sig.to_bytes())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Vsock port 1024 (arbitrary)
    let addr = VsockAddr::new(VMADDR_CID_ANY, 1024);
    let listener = VsockListener::bind(addr)?;
    println!("[enclave] listening on {:?}", addr);

    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut rng = OsRng;
            // Read command from client
            let mut cmd_buf = Vec::new();
            if stream.read_to_end(&mut cmd_buf).is_err() {
                return;
            }

            let cmd = String::from_utf8_lossy(&cmd_buf);

            let response = match cmd.trim() {
                "create" => {
                    let (sk_hex, pk_hex, addr) = generate_eth_wallet(&mut rng);
                    let eth = Wallet::Eth {
                        private_key: sk_hex,
                        public_key: pk_hex,
                        address: addr,
                    };
                    let (sk_b58, pk_b58) = generate_sol_wallet(&mut rng);
                    let sol = Wallet::Sol {
                        private_key: sk_b58,
                        public_key: pk_b58,
                    };
                    serde_json::to_vec(&vec![eth, sol]).unwrap()
                }
                s if s.starts_with("sign_eth:") => {
                    // sign_eth:<hex_key>:<message_hex>
                    let parts: Vec<&str> = s[9..].splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let msg = hex::decode(parts[1]).unwrap_or_default();
                        match sign_eth_message(parts[0], &msg) {
                            Ok(sig) => sig.into_bytes(),
                            Err(_) => b"error".to_vec(),
                        }
                    } else {
                        b"invalid".to_vec()
                    }
                }
                s if s.starts_with("sign_sol:") => {
                    // sign_sol:<b58_key>:<message>
                    let parts: Vec<&str> = s[8..].splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let sig = sign_sol_message(parts[0], parts[1].as_bytes());
                        sig.into_bytes()
                    } else {
                        b"invalid".to_vec()
                    }
                }
                _ => b"unknown command".to_vec(),
            };

            let _ = stream.write_all(&response).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_eth_wallet_structure() {
        let mut rng = OsRng;
        let (private_key, public_key, address) = generate_eth_wallet(&mut rng);
        // private key hex length 32 bytes => 64 hex chars
        assert_eq!(private_key.len(), 64);
        // public key uncompressed is 65 bytes => 130 hex chars
        assert_eq!(public_key.len(), 130);
        // address prefixed with 0x + 40 hex chars
        assert!(address.starts_with("0x"));
        assert_eq!(address.len(), 42);
    }

    #[test]
    fn test_sol_wallet_structure() {
        let mut rng = OsRng;
        let (private_key, public_key) = generate_sol_wallet(&mut rng);

        // secret key bytes = 32 => Base58 length ~44
        let sk_bytes = bs58::decode(private_key).into_vec().unwrap();
        assert_eq!(sk_bytes.len(), 32);
        // public key bytes = 32
        let pk_bytes = bs58::decode(public_key).into_vec().unwrap();
        assert_eq!(pk_bytes.len(), 32);
    }

    #[test]
    fn test_serialize_wallets_array() {
        let mut rng = OsRng;
        // Generate ETH and SOL wallets
        let (eth_sk, eth_pk, eth_addr) = generate_eth_wallet(&mut rng);
        let eth = Wallet::Eth {
            private_key: eth_sk,
            public_key: eth_pk,
            address: eth_addr,
        };
        let (sol_sk, sol_pk) = generate_sol_wallet(&mut rng);
        let sol = Wallet::Sol {
            private_key: sol_sk,
            public_key: sol_pk,
        };

        // Serialize to JSON
        let json = serde_json::to_string(&vec![eth, sol]).expect("serialization failed");
        // Parse as JSON Value
        let v: Value = serde_json::from_str(&json).expect("parse failed");
        let arr = v.as_array().expect("expected JSON array");
        assert_eq!(arr.len(), 2);
        // Check first element has expected fields
        let first = &arr[0];
        assert!(first.get("coin").is_some());
        assert_eq!(first.get("coin").unwrap(), "eth");
        assert!(first.get("private_key").is_some());
        assert!(first.get("public_key").is_some());
        assert!(first.get("address").is_some());
    }
}
