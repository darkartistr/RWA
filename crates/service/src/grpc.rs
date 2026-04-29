use crate::api::AppState;
use rwa_proto::rwa::token::v1::{
    token_service_server::TokenService, BurnRequest, BurnResponse, CreateMintRequest,
    CreateMintResponse, MintToRequest, MintToResponse, TransferRequest, TransferResponse,
};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct TokenGrpcService {
    state: AppState,
}

impl TokenGrpcService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl TokenService for TokenGrpcService {
    async fn create_mint(
        &self,
        request: Request<CreateMintRequest>,
    ) -> Result<Response<CreateMintResponse>, Status> {
        let req = request.into_inner();
        let decimals = u8::try_from(req.decimals).map_err(|_| Status::invalid_argument("decimals"))?;

        let (mint, sig) = self
            .state
            .sol
            .create_mint(decimals)
            .map_err(|e| Status::internal(e.to_string()))?;

        sqlx::query("insert into token_mints (mint, decimals) values ($1, $2) on conflict do nothing")
            .bind(mint.to_string())
            .bind(i32::from(decimals))
            .execute(self.state.db.pool())
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;

        let _ = sqlx::query(
            "insert into token_operations (op_type, mint, amount, sender, recipient, signature, status, error)
             values ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind("CREATE_MINT")
        .bind(mint.to_string())
        .bind("0")
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(Some(sig.to_string()))
        .bind("SUCCESS")
        .bind(Option::<String>::None)
        .execute(self.state.db.pool())
        .await;

        self.state
            .metrics
            .token_ops
            .with_label_values(&["create_mint", "success"])
            .inc();

        Ok(Response::new(CreateMintResponse {
            mint: mint.to_string(),
            signature: sig.to_string(),
        }))
    }

    async fn mint_to(
        &self,
        request: Request<MintToRequest>,
    ) -> Result<Response<MintToResponse>, Status> {
        let req = request.into_inner();
        let mint = Pubkey::from_str(&req.mint).map_err(|_| Status::invalid_argument("mint"))?;
        let recipient =
            Pubkey::from_str(&req.recipient_owner).map_err(|_| Status::invalid_argument("recipient_owner"))?;

        let (ata, sig) = self
            .state
            .sol
            .mint_to(mint, recipient, req.amount)
            .map_err(|e| Status::internal(e.to_string()))?;

        let _ = sqlx::query(
            "insert into token_operations (op_type, mint, amount, sender, recipient, signature, status, error)
             values ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind("MINT_TO")
        .bind(req.mint.clone())
        .bind(req.amount.to_string())
        .bind(Option::<String>::None)
        .bind(Some(req.recipient_owner.clone()))
        .bind(Some(sig.to_string()))
        .bind("SUCCESS")
        .bind(Option::<String>::None)
        .execute(self.state.db.pool())
        .await;

        let event = serde_json::json!({
            "type": "MINT_TO",
            "mint": req.mint,
            "recipient_owner": req.recipient_owner,
            "recipient_ata": ata.to_string(),
            "amount": req.amount.to_string(),
            "signature": sig.to_string(),
            "ts": chrono::Utc::now().to_rfc3339(),
        });
        let _ = self.state.kafka.publish_json(&sig.to_string(), &event).await;

        self.state
            .metrics
            .token_ops
            .with_label_values(&["mint_to", "success"])
            .inc();

        Ok(Response::new(MintToResponse {
            signature: sig.to_string(),
            recipient_ata: ata.to_string(),
        }))
    }

    async fn transfer(
        &self,
        request: Request<TransferRequest>,
    ) -> Result<Response<TransferResponse>, Status> {
        let req = request.into_inner();
        let mint = Pubkey::from_str(&req.mint).map_err(|_| Status::invalid_argument("mint"))?;
        let sender =
            Pubkey::from_str(&req.sender_owner).map_err(|_| Status::invalid_argument("sender_owner"))?;
        let recipient =
            Pubkey::from_str(&req.recipient_owner).map_err(|_| Status::invalid_argument("recipient_owner"))?;

        let ((sender_ata, recipient_ata), sig) = self
            .state
            .sol
            .transfer(mint, sender, recipient, req.amount)
            .map_err(|e| Status::internal(e.to_string()))?;

        let _ = sqlx::query(
            "insert into token_operations (op_type, mint, amount, sender, recipient, signature, status, error)
             values ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind("TRANSFER")
        .bind(req.mint.clone())
        .bind(req.amount.to_string())
        .bind(Some(req.sender_owner.clone()))
        .bind(Some(req.recipient_owner.clone()))
        .bind(Some(sig.to_string()))
        .bind("SUCCESS")
        .bind(Option::<String>::None)
        .execute(self.state.db.pool())
        .await;

        let event = serde_json::json!({
            "type": "TRANSFER",
            "mint": req.mint,
            "sender_owner": req.sender_owner,
            "recipient_owner": req.recipient_owner,
            "sender_ata": sender_ata.to_string(),
            "recipient_ata": recipient_ata.to_string(),
            "amount": req.amount.to_string(),
            "signature": sig.to_string(),
            "ts": chrono::Utc::now().to_rfc3339(),
        });
        let _ = self.state.kafka.publish_json(&sig.to_string(), &event).await;

        self.state
            .metrics
            .token_ops
            .with_label_values(&["transfer", "success"])
            .inc();

        Ok(Response::new(TransferResponse {
            signature: sig.to_string(),
            sender_ata: sender_ata.to_string(),
            recipient_ata: recipient_ata.to_string(),
        }))
    }

    async fn burn(&self, request: Request<BurnRequest>) -> Result<Response<BurnResponse>, Status> {
        let req = request.into_inner();
        let mint = Pubkey::from_str(&req.mint).map_err(|_| Status::invalid_argument("mint"))?;
        let owner = Pubkey::from_str(&req.owner).map_err(|_| Status::invalid_argument("owner"))?;

        let (ata, sig) = self
            .state
            .sol
            .burn(mint, owner, req.amount)
            .map_err(|e| Status::internal(e.to_string()))?;

        let _ = sqlx::query(
            "insert into token_operations (op_type, mint, amount, sender, recipient, signature, status, error)
             values ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind("BURN")
        .bind(req.mint.clone())
        .bind(req.amount.to_string())
        .bind(Some(req.owner.clone()))
        .bind(Option::<String>::None)
        .bind(Some(sig.to_string()))
        .bind("SUCCESS")
        .bind(Option::<String>::None)
        .execute(self.state.db.pool())
        .await;

        let event = serde_json::json!({
            "type": "BURN",
            "mint": req.mint,
            "owner": req.owner,
            "owner_ata": ata.to_string(),
            "amount": req.amount.to_string(),
            "signature": sig.to_string(),
            "ts": chrono::Utc::now().to_rfc3339(),
        });
        let _ = self.state.kafka.publish_json(&sig.to_string(), &event).await;

        self.state
            .metrics
            .token_ops
            .with_label_values(&["burn", "success"])
            .inc();

        Ok(Response::new(BurnResponse {
            signature: sig.to_string(),
            owner_ata: ata.to_string(),
        }))
    }
}

