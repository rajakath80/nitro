use actix_web::{App, HttpResponse, HttpServer, Responder, post, web::Json};
use hex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_vsock::{VsockAddr, VsockStream};

const ENCLAVE_CID: u32 = 19;
const ENCLAVE_PORT: u32 = 1024;
// const PROXY_HOST: &str = "13.221.31.204";
// const PROXY_PORT: u16 = 8080;

#[derive(Deserialize)]
struct CreateWallet {}

/// Wallet shape returned from enclave
#[derive(Serialize, Deserialize)]
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

/// Helper to talk directly to the enclave via vsock
async fn send_to_enclave(cmd: &str) -> anyhow::Result<Vec<u8>> {
    // connect via vsock to enclave CID and port
    let addr = VsockAddr::new(ENCLAVE_CID, ENCLAVE_PORT);
    let mut stream = VsockStream::connect(addr).await?;

    // send command bytes
    stream.write_all(cmd.as_bytes()).await?;

    // read full response
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    Ok(buf)
}

#[post("/wallet/create")]
async fn create_wallet() -> impl Responder {
    println!("Inside create_wallet..");

    match send_to_enclave("create").await {
        Ok(raw) => {
            println!("Raw: {:?}", raw);
            // parse JSON array of Wallet
            match serde_json::from_slice::<Vec<Wallet>>(&raw) {
                Ok(wallets) => HttpResponse::Ok().json(wallets),
                Err(e) => HttpResponse::InternalServerError()
                    .body(format!("Invalid JSON from enclave: {}", e)),
            }
        }
        Err(e) => {
            HttpResponse::InternalServerError().body(format!("Failed to talk to enclave: {}", e))
        }
    }
}
#[derive(Deserialize)]
struct SignEthRequest {
    private_key: String,
    message_hex: String,
}

#[post("/wallet/sign_eth")]
async fn sign_eth(req: Json<SignEthRequest>) -> impl Responder {
    let cmd = format!("sign_eth:{}:{}", req.private_key, req.message_hex);
    match send_to_enclave(&cmd).await {
        Ok(raw) => {
            // raw is signature bytes
            let sig_hex = hex::encode(&raw);
            HttpResponse::Ok().json(json!({ "signature": sig_hex }))
        }
        Err(e) => HttpResponse::InternalServerError().body(format!("Error signing ETH: {}", e)),
    }
}

#[derive(Deserialize)]
struct SignSolRequest {
    private_key: String,
    message: String,
}

#[post("/wallet/sign_sol")]
async fn sign_sol(req: Json<SignSolRequest>) -> impl Responder {
    let cmd = format!("sign_sol:{}:{}", req.private_key, req.message);
    match send_to_enclave(&cmd).await {
        Ok(raw) => {
            // raw is signature bytes
            let sig_hex = hex::encode(&raw);
            HttpResponse::Ok().json(json!({ "signature": sig_hex }))
        }
        Err(e) => HttpResponse::InternalServerError().body(format!("Error signing SOL: {}", e)),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Backend listening on http://0.0.0.0:8080");
    HttpServer::new(|| {
        App::new()
            .service(create_wallet)
            .service(sign_eth)
            .service(sign_sol)
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test};

    #[actix_rt::test]
    async fn test_create_wallet_endpoint() {
        let app = test::init_service(App::new().service(create_wallet)).await;
        let req = test::TestRequest::post().uri("/wallet/create").to_request();
        let resp = test::call_service(&app, req).await;
        // Since enclave may not be running in test, we expect a 500 response
        assert!(resp.status().is_server_error());
    }
}
