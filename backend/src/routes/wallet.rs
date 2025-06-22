use actix_web::{
    HttpResponse, Responder, post,
    web::{Data, Json},
};
use anyhow::Result;
use hex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, net::Shutdown, sync::Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_vsock::{VsockAddr, VsockStream};

pub const ENCLAVE_CID: u32 = 19;
pub const ENCLAVE_PORT: u32 = 1024;
// const PROXY_HOST: &str = "13.221.31.204";
// const PROXY_PORT: u16 = 8080;

/// In-memory store: email -> (pin, wallets)
pub type WalletStore = HashMap<String, (String, Vec<Wallet>)>;

#[derive(Deserialize, Serialize, Debug)]
pub struct CreateWalletRequest {
    pub email: String,
    pub pin: String,
}

/// Wallet shape returned from enclave
#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Deserialize, Serialize, Debug)]
pub struct SignEthRequest {
    pub email: String,
    pub pin: String,
    pub message: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct SignSolRequest {
    pub email: String,
    pub pin: String,
    pub message: String,
}

/// Register wallet-related routes
pub fn init_routes(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.service(create_wallet)
        .service(sign_eth)
        .service(sign_sol);
}

/// Helper to talk directly to the enclave via vsock
pub async fn send_to_enclave(cmd: &str) -> Result<Vec<u8>> {
    // connect via vsock to enclave CID and port
    let addr = VsockAddr::new(ENCLAVE_CID, ENCLAVE_PORT);
    let mut stream = VsockStream::connect(addr).await?;

    // send the command
    stream.write_all(cmd.as_bytes()).await?;
    // tell the enclave we're done sending
    stream.shutdown(Shutdown::Write)?;
    // collect the response
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    Ok(buf)
}

#[post("/wallet/create")]
async fn create_wallet(
    req: Json<CreateWalletRequest>,
    data: Data<Mutex<WalletStore>>,
) -> impl Responder {
    println!("Inside create_wallet.. {:?}", req);

    match send_to_enclave("create").await {
        Ok(raw) => {
            println!("Raw: {:?}", raw);
            // parse JSON array of Wallet
            match serde_json::from_slice::<Vec<Wallet>>(&raw) {
                Ok(wallets) => {
                    let mut store = data.lock().unwrap();
                    store.insert(req.email.clone(), (req.pin.clone(), wallets.clone()));
                    HttpResponse::Ok().json(wallets)
                }
                Err(e) => HttpResponse::InternalServerError()
                    .body(format!("Invalid JSON from enclave: {}", e)),
            }
        }
        Err(e) => {
            HttpResponse::InternalServerError().body(format!("Failed to talk to enclave: {}", e))
        }
    }
}

/// Internal helper to sign ETH messages
pub async fn sign_eth_inner(private_key: &str, message_hex: &str) -> Result<String, anyhow::Error> {
    let cmd = format!("sign_eth:{}:{}", private_key, message_hex);
    let raw = send_to_enclave(&cmd).await?;
    Ok(hex::encode(raw))
}

/// Internal helper to sign SOL messages
pub async fn sign_sol_inner(private_key: &str, message: &str) -> Result<String, anyhow::Error> {
    let cmd = format!("sign_sol:{}:{}", private_key, message);
    let raw = send_to_enclave(&cmd).await?;
    Ok(hex::encode(raw))
}

#[post("/wallet/sign_eth")]
pub async fn sign_eth(req: Json<SignEthRequest>, data: Data<Mutex<WalletStore>>) -> impl Responder {
    let store = data.lock().unwrap();
    if let Some((pin, ws)) = store.get(&req.email) {
        if pin == &req.pin {
            if let Some(private_key) = ws.iter().find_map(|w| {
                if let Wallet::Eth { private_key, .. } = w {
                    Some(private_key)
                } else {
                    None
                }
            }) {
                match sign_eth_inner(private_key, &req.message).await {
                    Ok(sig_hex) => return HttpResponse::Ok().json(json!({ "signature": sig_hex })),
                    Err(e) => {
                        return HttpResponse::InternalServerError()
                            .body(format!("Signing error: {}", e));
                    }
                }
            }
            return HttpResponse::BadRequest().body("No ETH wallet");
        }
    }
    HttpResponse::Unauthorized().body("Invalid credentials")
}

#[post("/wallet/sign_sol")]
pub async fn sign_sol(req: Json<SignSolRequest>, data: Data<Mutex<WalletStore>>) -> impl Responder {
    let store = data.lock().unwrap();
    if let Some((pin, ws)) = store.get(&req.email) {
        if pin == &req.pin {
            if let Some(private_key) = ws.iter().find_map(|w| {
                if let Wallet::Sol { private_key, .. } = w {
                    Some(private_key)
                } else {
                    None
                }
            }) {
                match sign_sol_inner(private_key, &req.message).await {
                    Ok(sig_hex) => return HttpResponse::Ok().json(json!({ "signature": sig_hex })),
                    Err(e) => {
                        return HttpResponse::InternalServerError()
                            .body(format!("Signing error: {}", e));
                    }
                }
            }
            return HttpResponse::BadRequest().body("No SOL wallet");
        }
    }
    HttpResponse::Unauthorized().body("Invalid credentials")
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::web::Data;
    use actix_web::{App, test};
    use std::{collections::HashMap, sync::Mutex};

    #[actix_rt::test]
    async fn create_wallet_returns_error_without_enclave() {
        let store = Data::new(Mutex::new(HashMap::<String, (String, Vec<Wallet>)>::new()));
        let app =
            test::init_service(App::new().app_data(store.clone()).service(create_wallet)).await;
        let payload = CreateWalletRequest {
            email: "x@x.com".into(),
            pin: "0000".into(),
        };
        let req = test::TestRequest::post()
            .uri("/wallet/create")
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_server_error());
    }

    #[actix_rt::test]
    async fn sign_eth_unauthorized_if_no_wallet() {
        let store = Data::new(Mutex::new(HashMap::<String, (String, Vec<Wallet>)>::new()));
        let app = test::init_service(App::new().app_data(store.clone()).service(sign_eth)).await;
        let payload = SignEthRequest {
            email: "x@x.com".into(),
            pin: "0000".into(),
            message: "aaa".into(),
        };
        let req = test::TestRequest::post()
            .uri("/wallet/sign_eth")
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    #[actix_rt::test]
    async fn sign_sol_unauthorized_if_no_wallet() {
        let store = Data::new(Mutex::new(HashMap::<String, (String, Vec<Wallet>)>::new()));
        let app = test::init_service(App::new().app_data(store.clone()).service(sign_sol)).await;
        let payload = SignSolRequest {
            email: "x@x.com".into(),
            pin: "0000".into(),
            message: "bbb".into(),
        };
        let req = test::TestRequest::post()
            .uri("/wallet/sign_sol")
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }
}
