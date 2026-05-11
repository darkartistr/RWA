use anyhow::Context;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use solana_system_interface::instruction as system_instruction;
use spl_associated_token_account::instruction as ata_ix;
use spl_token::instruction as token_ix;

#[derive(Clone)]
pub struct SolanaTokenClient {
    rpc: std::sync::Arc<RpcClient>,
    authority: std::sync::Arc<Keypair>,
}

impl SolanaTokenClient {
    pub fn new(rpc_url: &str, commitment: &str, keypair_path: &str) -> anyhow::Result<Self> {
        let commitment = match commitment {
            "processed" => CommitmentConfig::processed(),
            "confirmed" => CommitmentConfig::confirmed(),
            "finalized" => CommitmentConfig::finalized(),
            _ => CommitmentConfig::confirmed(),
        };

        let rpc = RpcClient::new_with_commitment(rpc_url.to_string(), commitment);
        let authority = read_keypair_file(keypair_path).map_err(|e| {
            anyhow::anyhow!("read keypair at {keypair_path}: {e}")
        })?;
        Ok(Self {
            rpc: std::sync::Arc::new(rpc),
            authority: std::sync::Arc::new(authority),
        })
    }

    pub fn authority_pubkey(&self) -> Pubkey {
        self.authority.pubkey()
    }

    pub async fn create_mint(&self, decimals: u8) -> anyhow::Result<(Pubkey, Signature)> {
        let mint = Keypair::new();
        let mint_pubkey = mint.pubkey();

        let rent = self
            .rpc
            .get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)
            .await
            .context("get rent exemption")?;

        let recent = self.rpc.get_latest_blockhash().await.context("get blockhash")?;

        let create_account = system_instruction::create_account(
            &self.authority.pubkey(),
            &mint_pubkey,
            rent,
            spl_token::state::Mint::LEN as u64,
            &spl_token::id(),
        );

        let init_mint = token_ix::initialize_mint2(
            &spl_token::id(),
            &mint_pubkey,
            &self.authority.pubkey(),
            Some(&self.authority.pubkey()),
            decimals,
        )
        .context("initialize mint")?;

        let tx = Transaction::new_signed_with_payer(
            &[create_account, init_mint],
            Some(&self.authority.pubkey()),
            &[self.authority.as_ref(), &mint],
            recent,
        );

        let sig = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .await
            .context("send create mint tx")?;

        Ok((mint_pubkey, sig))
    }

    pub async fn mint_to(
        &self,
        mint: Pubkey,
        recipient_owner: Pubkey,
        amount: u64,
    ) -> anyhow::Result<(Pubkey, Signature)> {
        let ata = spl_associated_token_account::get_associated_token_address(&recipient_owner, &mint);

        let mut ixs = vec![ata_ix::create_associated_token_account_idempotent(
            &self.authority.pubkey(),
            &recipient_owner,
            &mint,
            &spl_token::id(),
        )];

        ixs.push(
            token_ix::mint_to(
                &spl_token::id(),
                &mint,
                &ata,
                &self.authority.pubkey(),
                &[],
                amount,
            )
            .context("mint_to ix")?,
        );

        let recent = self.rpc.get_latest_blockhash().await.context("get blockhash")?;
        let tx = Transaction::new_signed_with_payer(
            &ixs,
            Some(&self.authority.pubkey()),
            &[self.authority.as_ref()],
            recent,
        );

        let sig = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .await
            .context("send mint_to tx")?;

        Ok((ata, sig))
    }

    pub async fn transfer(
        &self,
        mint: Pubkey,
        sender_owner: Pubkey,
        recipient_owner: Pubkey,
        amount: u64,
    ) -> anyhow::Result<((Pubkey, Pubkey), Signature)> {
        let sender_ata = spl_associated_token_account::get_associated_token_address(&sender_owner, &mint);
        let recipient_ata =
            spl_associated_token_account::get_associated_token_address(&recipient_owner, &mint);

        let mut ixs = vec![ata_ix::create_associated_token_account_idempotent(
            &self.authority.pubkey(),
            &recipient_owner,
            &mint,
            &spl_token::id(),
        )];

        ixs.push(
            token_ix::transfer(
                &spl_token::id(),
                &sender_ata,
                &recipient_ata,
                &self.authority.pubkey(),
                &[],
                amount,
            )
            .context("transfer ix")?,
        );

        let recent = self.rpc.get_latest_blockhash().await.context("get blockhash")?;
        let tx = Transaction::new_signed_with_payer(
            &ixs,
            Some(&self.authority.pubkey()),
            &[self.authority.as_ref()],
            recent,
        );

        let sig = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .await
            .context("send transfer tx")?;

        Ok(((sender_ata, recipient_ata), sig))
    }

    pub async fn burn(
        &self,
        mint: Pubkey,
        owner: Pubkey,
        amount: u64,
    ) -> anyhow::Result<(Pubkey, Signature)> {
        let ata = spl_associated_token_account::get_associated_token_address(&owner, &mint);

        let ix = token_ix::burn(
            &spl_token::id(),
            &ata,
            &mint,
            &self.authority.pubkey(),
            &[],
            amount,
        )
        .context("burn ix")?;

        let recent = self.rpc.get_latest_blockhash().await.context("get blockhash")?;
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&self.authority.pubkey()),
            &[self.authority.as_ref()],
            recent,
        );

        let sig = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .await
            .context("send burn tx")?;

        Ok((ata, sig))
    }
}

