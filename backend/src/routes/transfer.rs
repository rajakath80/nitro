use crate::routes::wallet::{Wallet, WalletStore, sign_sol_inner};
use actix_web::{
    HttpResponse, Responder, post,
    web::{self, Data},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig, message::Message, pubkey::Pubkey, signature::Signature,
    system_instruction, transaction::Transaction,
};
use std::{str::FromStr, sync::Mutex};

/// Request payload for SOL transfers
#[derive(Deserialize, Serialize, Debug)]
pub struct TransferRequest {
    pub email: String,
    pub pin: String,
    pub recipient: String,
    pub amount: u64,
}

/// Response payload containing the transaction signature
#[derive(Serialize, Debug)]
pub struct TransferResponse {
    pub tx_signature: String,
}

/// Wallet transfer endpoint
#[post("/wallet/transfer/solana")]
pub async fn transfer_sol(
    req: web::Json<TransferRequest>,
    data: Data<Mutex<WalletStore>>,
) -> impl Responder {
    println!("Inside transfer_sol.. {:?}", req);

    // Authenticate & extract SOL keys
    let (pubkey_hex, privkey_hex) = match data.lock().unwrap().get(&req.email) {
        Some((pin, ws)) if pin == &req.pin => {
            // find the Sol wallet entry
            if let Some((public_key, private_key)) = ws.iter().find_map(|w| {
                if let Wallet::Sol {
                    private_key,
                    public_key,
                } = w.clone()
                {
                    Some((public_key, private_key))
                } else {
                    None
                }
            }) {
                (public_key, private_key)
            } else {
                return HttpResponse::Unauthorized().body("No SOL wallet for this user");
            }
        }
        _ => return HttpResponse::Unauthorized().body("Invalid credentials or no SOL wallet"),
    };

    println!("Found pub & priv key {}", pubkey_hex);

    // Build message
    let sender = Pubkey::from_str(&pubkey_hex)
        .map_err(|e| HttpResponse::BadRequest().body(format!("Invalud sender pubkey : {}", e)))
        .unwrap();

    let recipient = Pubkey::from_str(&req.recipient)
        .map_err(|e| HttpResponse::BadRequest().body(format!("Invalid recipient: {}", e)))
        .unwrap();

    println!("Creating system instruction..");

    let ix = system_instruction::transfer(&sender, &recipient, req.amount);
    let message = Message::new(&[ix.clone()], Some(&sender));
    let msg_bytes = message.serialize();
    let msg_hex = hex::encode(&msg_bytes);

    println!("Created msg for signing .. {}", msg_hex);

    // Sign via internal function
    let sign_hex = match sign_sol_inner(&privkey_hex, &msg_hex).await {
        Ok(s) => s,
        Err(e) => return HttpResponse::InternalServerError().body(format!("Signin error: {}", e)),
    };

    let sign_bytes = hex::decode(&sign_hex)
        .map_err(|e| HttpResponse::InternalServerError().body(format!("Hex decode error: {}", e)))
        .unwrap();

    let sig_arr: [u8; 64] = match sign_bytes.as_slice().try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return HttpResponse::InternalServerError()
                .body(format!("Unexpected signature length: {}", sign_bytes.len()));
        }
    };

    println!("Creating transaction ..");

    let signature = Signature::from(sig_arr);

    // Send to Devnet
    let rpc = RpcClient::new_with_commitment(
        "https://api.devnet.solana.com".to_string(),
        CommitmentConfig::confirmed(),
    );

    let recent_blockhash = rpc
        .get_latest_blockhash()
        .await
        .map_err(|e| HttpResponse::InternalServerError().body(format!("Blockhash error: {}", e)))
        .unwrap();

    // Rebuild the Message *with* that blockhash
    let tx_message = Message::new_with_blockhash(
        &[ix.clone()],     // your transfer instruction
        Some(&sender),     // fee payer
        &recent_blockhash, // fresh blockhash
    );

    // Assemble transaction
    let tx = Transaction {
        signatures: vec![signature],
        message: tx_message,
    };

    println!("Sending to Devnet RPC ..");

    match rpc.send_and_confirm_transaction(&tx).await {
        Ok(tx_sig) => HttpResponse::Ok().json(json!({"tx_signature": tx_sig.to_string()})),
        Err(e) => HttpResponse::InternalServerError().body(format!("RPC error: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::web::Data;
    use actix_web::{App, test};
    use std::{collections::HashMap, sync::Mutex};

    #[actix_rt::test]
    async fn test_transfer_unauthorized() {
        let store = Data::new(Mutex::new(HashMap::<String, (String, Vec<Wallet>)>::new()));
        let app =
            test::init_service(App::new().app_data(store.clone()).service(transfer_sol)).await;
        let payload = TransferRequest {
            email: "a@example.com".into(),
            pin: "1234".into(),
            recipient: "invalid".into(),
            amount: 1,
        };
        let req = test::TestRequest::post()
            .uri("/wallet/transfer")
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }
}
