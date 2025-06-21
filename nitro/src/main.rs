use bs58;
use ed25519_dalek::SigningKey as EdSigningKey;
use hex;
use k256::ecdsa::SigningKey;
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
fn generate_eth_wallet(rng: &mut OsRng) -> Wallet {
    let sk = SigningKey::random(rng);
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

    Wallet::Eth {
        private_key: sk_hex,
        public_key: hex::encode(pk_bytes),
        address: format!("0x{}", address),
    }
}

fn bytes_to_base58_string(bytes: &[u8]) -> String {
    bs58::encode(bytes).into_string()
}

/// Generate a Solana wallet (ed25519-based)
fn generate_sol_wallet(rng: &mut OsRng) -> Wallet {
    // ed25519_dalek::Keypair is now imported directly as Keypair
    let sk = EdSigningKey::generate(rng);
    let vk = sk.verifying_key();

    // Extract bytes for secret and public keys
    let sk_bytes = sk.to_bytes(); // [u8; 32]
    let pk_bytes = vk.to_bytes(); // [u8; 32]

    // Encode to Base58 or hex as desired
    let sk_bs58 = bytes_to_base58_string(&sk_bytes);
    let pk_bs58 = bytes_to_base58_string(&pk_bytes);

    Wallet::Sol {
        private_key: sk_bs58,
        public_key: pk_bs58,
    }
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
            let eth_wallet = generate_eth_wallet(&mut rng);
            let sol_wallet = generate_sol_wallet(&mut rng);

            let resp = serde_json::to_vec(&[eth_wallet, sol_wallet]).unwrap();
            println!("Enclave Response: {}", String::from_utf8_lossy(&resp));
            let _ = stream.write_all(&resp).await;
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
        if let Wallet::Eth {
            private_key,
            public_key,
            address,
        } = generate_eth_wallet(&mut rng)
        {
            // private key hex length 32 bytes => 64 hex chars
            assert_eq!(private_key.len(), 64);
            // public key uncompressed is 65 bytes => 130 hex chars
            assert_eq!(public_key.len(), 130);
            // address prefixed with 0x + 40 hex chars
            assert!(address.starts_with("0x"));
            assert_eq!(address.len(), 42);
        } else {
            panic!("Expected Eth variant");
        }
    }

    #[test]
    fn test_sol_wallet_structure() {
        let mut rng = OsRng;
        if let Wallet::Sol {
            private_key,
            public_key,
        } = generate_sol_wallet(&mut rng)
        {
            // secret key bytes = 32 => Base58 length ~44
            let sk_bytes = bs58::decode(private_key).into_vec().unwrap();
            assert_eq!(sk_bytes.len(), 32);
            // public key bytes = 32
            let pk_bytes = bs58::decode(public_key).into_vec().unwrap();
            assert_eq!(pk_bytes.len(), 32);
        } else {
            panic!("Expected Sol variant");
        }
    }

    #[test]
    fn test_serialize_wallets_array() {
        let mut rng = OsRng;
        let eth = generate_eth_wallet(&mut rng);
        let sol = generate_sol_wallet(&mut rng);
        let arr = vec![eth, sol];
        let json = serde_json::to_string(&arr).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // First element has coin field
        assert!(arr[0].get("coin").is_some());
    }
}
