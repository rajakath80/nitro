mod routes {
    pub mod transfer;
    pub mod wallet;
}

use actix_web::{App, HttpServer, web::Data};
use routes::wallet;
use std::{collections::HashMap, sync::Mutex};

use crate::routes::{transfer::transfer_sol, wallet::WalletStore};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Backend listening on http://0.0.0.0:8080");
    let store: Data<Mutex<WalletStore>> = Data::new(Mutex::new(HashMap::new()));
    HttpServer::new(move || {
        App::new()
            .app_data(store.clone())
            .configure(wallet::init_routes)
            .service(transfer_sol)
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::wallet::CreateWalletRequest;
    use actix_web::web::Data;
    use actix_web::{App, test};
    use std::{collections::HashMap, sync::Mutex};

    #[actix_rt::test]
    async fn test_routes_registered() {
        let store: Data<Mutex<WalletStore>> = Data::new(Mutex::new(HashMap::new()));
        let app = test::init_service(
            App::new()
                .app_data(store.clone())
                .configure(wallet::init_routes)
                .service(transfer_sol),
        )
        .await;
        // POST to /wallet/create should exist and return server error
        let payload = CreateWalletRequest {
            email: "a@example.com".into(),
            pin: "1234".into(),
        };
        let req = test::TestRequest::post()
            .uri("/wallet/create")
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_server_error());
    }
}
