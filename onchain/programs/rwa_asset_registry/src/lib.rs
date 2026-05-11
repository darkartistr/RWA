use anchor_lang::prelude::*;

declare_id!("RWAReg1111111111111111111111111111111111");

#[program]
pub mod rwa_asset_registry {
    use super::*;

    pub fn initialize_asset(ctx: Context<InitializeAsset>, mint: Pubkey) -> Result<()> {
        let asset = &mut ctx.accounts.asset;
        asset.mint = mint;
        asset.admin = ctx.accounts.admin.key();
        asset.bump = ctx.bumps.asset;
        Ok(())
    }

    pub fn set_admin(ctx: Context<SetAdmin>, new_admin: Pubkey) -> Result<()> {
        let asset = &mut ctx.accounts.asset;
        asset.admin = new_admin;
        Ok(())
    }
}

#[account]
pub struct Asset {
    pub mint: Pubkey,
    pub admin: Pubkey,
    pub bump: u8,
}

impl Asset {
    pub const LEN: usize = 8 + 32 + 32 + 1;
}

#[derive(Accounts)]
#[instruction(mint: Pubkey)]
pub struct InitializeAsset<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = Asset::LEN,
        seeds = [b"asset", mint.as_ref()],
        bump
    )]
    pub asset: Account<'info, Asset>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetAdmin<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        has_one = admin,
    )]
    pub asset: Account<'info, Asset>,
}

