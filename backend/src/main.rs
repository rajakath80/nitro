use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use tokio_vsock::{VsockStream, VsockAddr, VMADDR_CID_ANY};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Deserialize)]
struct CreateWallet {}

#[derive(Deserialize, Serialize)]
struct Wallet {
    private_key: String,
    public_key: String,
}

#[post("/wallet/create")]
async fn create_wallet(_body: web::Json<CreateWallet>) -> impl Responder {
    //connect to enclace at CID=3, port=1024
    let addr = VsockAddr::new(VMADDR_CID_ANY, 1024);
    let mut stream = VsockStream::connect(addr).await.expect("vsock connect failed");

    stream.write_all(b"gen").await.unwrap();

    let mut buf = vec![0u8; 512];
    let n = stream.read(&mut buf).await.unwrap();
    let wallet: Wallet = serde_json::from_slice(&buf[..n]).unwrap();

    HttpResponse::Ok().json(wallet)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Listing on http://0.0.0.0:8080 ..");
    HttpServer::new(|| App::new().service(create_wallet)).bind("0.0.0.0:8080")?.run().await
}