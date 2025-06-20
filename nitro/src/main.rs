use k256::ecdsa::SigningKey;
use k256::elliptic_curve::rand_core::OsRng;
use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio_vsock::{VMADDR_CID_ANY, VsockAddr, VsockListener};

#[derive(Serialize)]
struct Wallet {
    private_key: String,
    public_key: String,
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
            // generate a new secp256k1 key pair
            let mut rng = OsRng;
            let signing_key = SigningKey::random(&mut rng);
            let verify_key = signing_key.verifying_key();

            let wallet = Wallet {
                private_key: hex::encode(signing_key.to_bytes()),
                public_key: hex::encode(verify_key.to_encoded_point(false).as_bytes()),
            };

            let resp = serde_json::to_vec(&wallet).unwrap();
            println!("Enclave Response: {:?}", resp);
            let _ = stream.write_all(&resp).await;
        });
    }
}
