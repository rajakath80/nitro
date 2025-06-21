use actix_web::{App, HttpResponse, HttpServer, Responder, post};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio_vsock::{VsockAddr, VsockStream};

const ENCLAVE_CID: u32 = 19;
const ENCLAVE_PORT: u32 = 1024;

#[derive(Deserialize)]
struct CreateWallet {}

#[derive(Deserialize, Serialize, Debug)]
struct Wallet {
    private_key: String,
    public_key: String,
}

#[post("/wallet/create")]
async fn create_wallet() -> impl Responder {
    println!("Inside create_wallet..");

    //connect to enclace at CID=xx, port=xxxx
    let addr = VsockAddr::new(ENCLAVE_CID, ENCLAVE_PORT);
    let mut stream = match VsockStream::connect(addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("vsock connect failed: {}", e);
            return HttpResponse::InternalServerError()
                .body(format!("failed to connect to enclave: {}", e));
        }
    };

    println!("Before read_to_end ..");
    // If enclave expects a command, write something here:
    // stream.write_all(b"gen").await.unwrap();

    // Read the JSON response in full
    let mut buf = Vec::new();
    if let Err(e) = stream.read_to_end(&mut buf).await {
        eprintln!("vsock read failed: {}", e);
        return HttpResponse::InternalServerError()
            .body(format!("failed to read from enclave: {}", e));
    }

    // Deserialize into your simple Wallet struct
    match serde_json::from_slice::<Wallet>(&buf) {
        Ok(wallet) => {
            println!("Received wallet: {:?}", wallet);
            HttpResponse::Ok().json(wallet)
        }
        Err(e) => {
            eprintln!("json parse error: {}", e);
            HttpResponse::InternalServerError().body(format!("invalid JSON from enclave: {}", e))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Listing on http://0.0.0.0:8080 ..");
    HttpServer::new(|| App::new().service(create_wallet))
        .bind("0.0.0.0:8080")?
        .run()
        .await
}
