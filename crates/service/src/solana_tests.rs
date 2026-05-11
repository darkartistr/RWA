use crate::solana::SolanaTokenClient;
use anyhow::{Context, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::{fs, path::PathBuf};
use tokio::time::{sleep, Duration};

const DEFAULT_RPC_URL: &str = "http://127.0.0.1:8899";

struct TestContext {
    client: SolanaTokenClient,
    rpc: RpcClient,
    authority: Pubkey,
    keypair_path: PathBuf,
}

impl TestContext {
    async fn new() -> Result<Self> {
        let rpc_url = std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC_URL.to_string());
        let commitment = std::env::var("SOLANA_COMMITMENT").unwrap_or_else(|_| "confirmed".to_string());

        let keypair_path = if let Ok(path) = std::env::var("SOLANA_TEST_KEYPAIR_PATH") {
            PathBuf::from(path)
        } else {
            let keypair = Keypair::new();
            let bytes: Vec<u8> = keypair.to_bytes().into();
            let path =
                std::env::temp_dir().join(format!("rwa-solana-test-keypair-{}.json", uuid::Uuid::new_v4()));
            fs::write(&path, serde_json::to_vec(&bytes).context("serialize temp keypair")?)
                .context("write temp keypair")?;
            path
        };

        let client = SolanaTokenClient::new(
            &rpc_url,
            &commitment,
            keypair_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("keypair path is not valid UTF-8"))?,
        )
        .context("create SolanaTokenClient for tests")?;
        let authority = client.authority_pubkey();
        let rpc = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());

        Ok(Self {
            client,
            rpc,
            authority,
            keypair_path,
        })
    }

    async fn airdrop_if_needed(&self, minimum_lamports: u64) -> Result<()> {
        let current = self
            .rpc
            .get_balance(&self.authority)
            .await
            .context("read authority SOL balance")?;
        if current >= minimum_lamports {
            return Ok(());
        }

        let need = minimum_lamports - current;
        let sig = self
            .rpc
            .request_airdrop(&self.authority, need)
            .await
            .context("request authority airdrop")?;

        let _ = self
            .rpc
            .confirm_transaction(&sig)
            .await
            .context("confirm authority airdrop")?;
        sleep(Duration::from_millis(300)).await;
        Ok(())
    }

    async fn token_amount(&self, ata: &Pubkey) -> Result<u64> {
        let amount = self
            .rpc
            .get_token_account_balance(ata)
            .await
            .context("get token account balance")?
            .amount
            .parse::<u64>()
            .context("parse token amount")?;
        Ok(amount)
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        if std::env::var("SOLANA_TEST_KEYPAIR_PATH").is_err() {
            let _ = fs::remove_file(&self.keypair_path);
        }
    }
}

async fn prepare_mint_with_tokens(initial_amount: u64) -> Result<(TestContext, Pubkey, Pubkey)> {
    let ctx = TestContext::new().await?;
    ctx.airdrop_if_needed(2_000_000_000).await?;
    let (mint, _) = ctx.client.create_mint(6).await?;
    let (authority_ata, _) = ctx.client.mint_to(mint, ctx.authority, initial_amount).await?;
    Ok((ctx, mint, authority_ata))
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn mint_to_creates_associated_account() -> Result<()> {
    let ctx = TestContext::new().await?;
    ctx.airdrop_if_needed(2_000_000_000).await?;
    let (mint, _) = ctx.client.create_mint(6).await?;
    let recipient = Pubkey::new_unique();

    let (ata, sig) = ctx.client.mint_to(mint, recipient, 10).await?;

    assert_ne!(sig.to_string(), "");
    assert_eq!(
        ata,
        spl_associated_token_account::get_associated_token_address(&recipient, &mint)
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn mint_to_writes_requested_amount() -> Result<()> {
    let ctx = TestContext::new().await?;
    ctx.airdrop_if_needed(2_000_000_000).await?;
    let (mint, _) = ctx.client.create_mint(6).await?;
    let recipient = Pubkey::new_unique();

    let (ata, _) = ctx.client.mint_to(mint, recipient, 123).await?;
    let amount = ctx.token_amount(&ata).await?;

    assert_eq!(amount, 123);
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn mint_to_accumulates_on_repeat_calls() -> Result<()> {
    let ctx = TestContext::new().await?;
    ctx.airdrop_if_needed(2_000_000_000).await?;
    let (mint, _) = ctx.client.create_mint(6).await?;
    let recipient = Pubkey::new_unique();

    let (ata, _) = ctx.client.mint_to(mint, recipient, 10).await?;
    let _ = ctx.client.mint_to(mint, recipient, 25).await?;
    let amount = ctx.token_amount(&ata).await?;

    assert_eq!(amount, 35);
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn mint_to_zero_amount_keeps_balance_zero() -> Result<()> {
    let ctx = TestContext::new().await?;
    ctx.airdrop_if_needed(2_000_000_000).await?;
    let (mint, _) = ctx.client.create_mint(6).await?;
    let recipient = Pubkey::new_unique();

    let (ata, _) = ctx.client.mint_to(mint, recipient, 0).await?;
    let amount = ctx.token_amount(&ata).await?;

    assert_eq!(amount, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn mint_to_fails_for_unknown_mint() -> Result<()> {
    let ctx = TestContext::new().await?;
    ctx.airdrop_if_needed(2_000_000_000).await?;
    let recipient = Pubkey::new_unique();
    let unknown_mint = Pubkey::new_unique();

    let err = ctx.client.mint_to(unknown_mint, recipient, 10).await.unwrap_err();
    assert!(
        err.to_string().contains("send mint_to tx")
            || err.to_string().contains("Transaction simulation failed")
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn transfer_moves_requested_amount() -> Result<()> {
    let (ctx, mint, sender_ata) = prepare_mint_with_tokens(200).await?;
    let recipient = Pubkey::new_unique();

    let ((from_ata, to_ata), _) = ctx.transfer(mint, ctx.authority, recipient, 75).await?;
    let sender_amount = ctx.token_amount(&sender_ata).await?;
    let recipient_amount = ctx.token_amount(&to_ata).await?;

    assert_eq!(from_ata, sender_ata);
    assert_eq!(sender_amount, 125);
    assert_eq!(recipient_amount, 75);
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn transfer_creates_recipient_ata() -> Result<()> {
    let (ctx, mint, _) = prepare_mint_with_tokens(100).await?;
    let recipient = Pubkey::new_unique();

    let ((_, recipient_ata), _) = ctx.transfer(mint, ctx.authority, recipient, 1).await?;

    assert_eq!(
        recipient_ata,
        spl_associated_token_account::get_associated_token_address(&recipient, &mint)
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn transfer_zero_amount_does_not_change_balances() -> Result<()> {
    let (ctx, mint, sender_ata) = prepare_mint_with_tokens(100).await?;
    let recipient = Pubkey::new_unique();

    let ((_, recipient_ata), _) = ctx.transfer(mint, ctx.authority, recipient, 0).await?;
    let sender_amount = ctx.token_amount(&sender_ata).await?;
    let recipient_amount = ctx.token_amount(&recipient_ata).await?;

    assert_eq!(sender_amount, 100);
    assert_eq!(recipient_amount, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn transfer_fails_with_wrong_sender_owner() -> Result<()> {
    let (ctx, mint, _) = prepare_mint_with_tokens(100).await?;
    let wrong_owner = Pubkey::new_unique();
    let recipient = Pubkey::new_unique();

    let err = ctx
        .transfer(mint, wrong_owner, recipient, 10)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("send transfer tx")
            || err.to_string().contains("Transaction simulation failed")
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn transfer_fails_when_amount_exceeds_balance() -> Result<()> {
    let (ctx, mint, _) = prepare_mint_with_tokens(5).await?;
    let recipient = Pubkey::new_unique();

    let err = ctx
        .transfer(mint, ctx.authority, recipient, 6)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("send transfer tx")
            || err.to_string().contains("Transaction simulation failed")
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn burn_reduces_balance() -> Result<()> {
    let (ctx, mint, authority_ata) = prepare_mint_with_tokens(120).await?;

    let (ata, _) = ctx.burn(mint, ctx.authority, 20).await?;
    let amount = ctx.token_amount(&authority_ata).await?;

    assert_eq!(ata, authority_ata);
    assert_eq!(amount, 100);
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn burn_zero_amount_keeps_balance() -> Result<()> {
    let (ctx, mint, authority_ata) = prepare_mint_with_tokens(77).await?;

    let _ = ctx.burn(mint, ctx.authority, 0).await?;
    let amount = ctx.token_amount(&authority_ata).await?;

    assert_eq!(amount, 77);
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn burn_fails_with_wrong_owner() -> Result<()> {
    let (ctx, mint, _) = prepare_mint_with_tokens(50).await?;
    let wrong_owner = Pubkey::new_unique();

    let err = ctx.burn(mint, wrong_owner, 10).await.unwrap_err();
    assert!(
        err.to_string().contains("send burn tx")
            || err.to_string().contains("Transaction simulation failed")
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn burn_fails_when_amount_exceeds_balance() -> Result<()> {
    let (ctx, mint, _) = prepare_mint_with_tokens(10).await?;

    let err = ctx.burn(mint, ctx.authority, 11).await.unwrap_err();
    assert!(
        err.to_string().contains("send burn tx")
            || err.to_string().contains("Transaction simulation failed")
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires funded account and reachable Solana RPC (local validator/devnet)"]
async fn burn_after_transfer_updates_sender_balance() -> Result<()> {
    let (ctx, mint, authority_ata) = prepare_mint_with_tokens(90).await?;
    let recipient = Pubkey::new_unique();

    let _ = ctx.transfer(mint, ctx.authority, recipient, 30).await?;
    let _ = ctx.burn(mint, ctx.authority, 20).await?;
    let amount = ctx.token_amount(&authority_ata).await?;

    assert_eq!(amount, 40);
    Ok(())
}

trait SolanaOps {
    async fn transfer(
        &self,
        mint: Pubkey,
        sender_owner: Pubkey,
        recipient_owner: Pubkey,
        amount: u64,
    ) -> Result<((Pubkey, Pubkey), solana_sdk::signature::Signature)>;
    async fn burn(
        &self,
        mint: Pubkey,
        owner: Pubkey,
        amount: u64,
    ) -> Result<(Pubkey, solana_sdk::signature::Signature)>;
}

impl SolanaOps for TestContext {
    async fn transfer(
        &self,
        mint: Pubkey,
        sender_owner: Pubkey,
        recipient_owner: Pubkey,
        amount: u64,
    ) -> Result<((Pubkey, Pubkey), solana_sdk::signature::Signature)> {
        self.client
            .transfer(mint, sender_owner, recipient_owner, amount)
            .await
    }

    async fn burn(
        &self,
        mint: Pubkey,
        owner: Pubkey,
        amount: u64,
    ) -> Result<(Pubkey, solana_sdk::signature::Signature)> {
        self.client.burn(mint, owner, amount).await
    }
}
