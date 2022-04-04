use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, TokenAccount, MintTo, Burn, Transfer, Token};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod flashloan {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let flashloan = &mut ctx.accounts.flashloan;

        flashloan.token_authority_bump = *ctx.bumps.get("token_authority").unwrap();
        flashloan.authority = ctx.accounts.authority.key();

        Ok(())
    }

    /// Add pool for a given token mint, setup a pool, token account and lp token mint
    pub fn add_pool(ctx: Context<AddPool>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        pool.bump = *ctx.bumps.get("pool").unwrap();
        pool.borrowing = false;
        pool.token_mint = ctx.accounts.token_mint.key();
        pool.pool_token = ctx.accounts.pool_token.key();
        pool.lp_token_mint = ctx.accounts.lp_token_mint.key();

        Ok(())
    }

    /// Receive tokens and mint lp tokens
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        Ok(())
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        Ok(())
    }

    pub fn mint_voucher(ctx: Context<MintVoucher>) -> Result<()> {
        Ok(())
    }

    pub fn borrow(ctx: Context<Borrow>, amount: u64) -> Result<()> {
        Ok(())
    }

    pub fn repay(ctx: Context<Repay>, amount: u64) -> Result<()> {
        Ok(())
    }
}

// ----------------------------------------------------------------------------

pub const FLASHLOAN_NAMESPACE: [u8; 9] = *b"flashloan";
pub const TOKEN_NAMESPACE: [u8; 5] = *b"token";
pub const LP_TOKEN_NAMESPACE: [u8; 14] = *b"liquidity_pool";

// ----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = FlashLoan::LEN,
    )]
    pub flashloan: Account<'info, FlashLoan>,

    #[account(
        seeds = [flashloan.key().as_ref(), FLASHLOAN_NAMESPACE.as_ref()],
        bump
    )]
    /// CHECK: Checked above, used only for bump calc
    pub token_authority: UncheckedAccount<'info>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AddPool<'info> {
    #[account(has_one = authority)]
    pub flashloan: Box<Account<'info, FlashLoan>>,

    #[account(
        seeds = [flashloan.key().as_ref(), FLASHLOAN_NAMESPACE.as_ref()],
        bump = flashloan.token_authority_bump
    )]
    /// CHECK: Checked above, used only for bump calc
    pub token_authority: UncheckedAccount<'info>,

    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = Pool::LEN,
        seeds = [flashloan.key().as_ref(), token_mint.key().as_ref()],
        bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        init,
        seeds = [flashloan.key().as_ref(), TOKEN_NAMESPACE.as_ref(), token_mint.key().as_ref()],
        bump,
        token::mint = token_mint,
        token::authority = token_authority,
        payer = payer,
    )]
    pub pool_token: Account<'info, TokenAccount>,

    #[account(
        init,
        seeds = [flashloan.key().as_ref(), LP_TOKEN_NAMESPACE.as_ref(), token_mint.key().as_ref()],
        bump,
        mint::authority = token_authority,
        mint::decimals = token_mint.decimals,
        payer = payer,
    )]
    pub lp_token_mint: Account<'info, Mint>,

    pub token_mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit {
}

#[derive(Accounts)]
pub struct Withdraw {
}

#[derive(Accounts)]
pub struct MintVoucher {}

#[derive(Accounts)]
pub struct Borrow {}

#[derive(Accounts)]
pub struct Repay {}

#[account]
pub struct FlashLoan {
    token_authority_bump: u8,
    authority: Pubkey,
}

impl FlashLoan {
    const LEN: usize = 8 + 32 + 1;
}

#[account]
pub struct Pool {
    bump: u8,
    borrowing: bool,
    token_mint: Pubkey,
    pool_token: Pubkey,
    lp_token_mint: Pubkey,
}

impl Pool {
    const LEN: usize = 8 + 2 + 32*3;
}