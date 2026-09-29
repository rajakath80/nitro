use anyhow::{Error, Result};
use ed25519_dalek::{
    Signature as SolSignature, Signer as SolSigner, SigningKey as SolSigningKey,
    VerifyingKey as SolVerifyingKey,
};
use k256::ecdsa::{Signature as EthSignature, SigningKey as EthSigningKey};
use k256::elliptic_curve::generic_array::GenericArray;
use k256::elliptic_curve::rand_core::OsRng;
use serde::{Deserialize, Serialize};
use tiny_keccak::{Hasher, Keccak};

// Represents a HD-less wallet
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "coin", rename_all = "lowercase")]
pub enum Wallet {
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

pub trait WalletProvider {
    fn create_wallets(&self) -> Result<Vec<Wallet>, Error>;
    fn sign_eth(&self, private_key: &str, message: &str) -> Result<String, Error>;
    fn sign_sol(&self, private_key: &str, message: &str) -> Result<String, Error>;
}

pub struct LocalProvider;
impl LocalProvider {
    pub fn new() -> Self {
        LocalProvider
    }
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
fn sign_eth_message(sk_hex: &str, message: &[u8]) -> Result<String, Error> {}

/// Sign a message with a Solana private key (base58)
fn sign_sol_message(sk_b58: &str, message: &[u8]) -> [u8; 64] {
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
    sig.to_bytes()
}

impl WalletProvider for LocalProvider {
    fn create_wallets(&self) -> Result<Vec<Wallet>, Error> {
        let mut rng = OsRng;

        // ETH wallet
        let (sk_hex, pk_hex, addr) = generate_eth_wallet(&mut rng);
        let eth = Wallet::Eth {
            private_key: sk_hex,
            public_key: pk_hex,
            address: addr,
        };

        // SOL wallet
        let (sk_b58, pk_b58) = generate_sol_wallet(&mut rng);
        let sol = Wallet::Sol {
            private_key: sk_b58,
            public_key: pk_b58,
        };

        Ok(vec![eth, sol])
    }

    fn sign_eth(&self, private_key: &str, message: &str) -> Result<String, Error> {
        // Decode hex into Vec<u8>, then convert to [u8; 32]
        let sk_vec = hex::decode(private_key).expect("Invalid hex for private key");
        let sk_arr: [u8; 32] = sk_vec
            .as_slice()
            .try_into()
            .expect("private key must be 32 bytes");
        // Convert to GenericArray for SigningKey
        let ga = GenericArray::clone_from_slice(&sk_arr);
        let sk = EthSigningKey::from_bytes(&ga)?;
        let msg_bytes = hex::decode(message)?;
        let sign = sk.sign(&msg_bytes);
        // let sig: EthSignature = sk.sign(hex::decode(message));
        Ok(hex::encode(sign.as_ref()))
    }

    fn sign_sol(&self, private_key: &str, message: &str) -> Result<String, Error> {
        sign_sol_message(hex::decode(private_key), message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
