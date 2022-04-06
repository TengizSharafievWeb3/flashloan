use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, TokenAccount, MintTo, Burn, Transfer, Token};
use anchor_lang::solana_program::sysvar::instructions;
use std::convert::TryInto;
use sha2_const::Sha256;

mod calc;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod flashloan {
    use crate::calc::{shares_from_value, value_from_shares};
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, basis_points: u32) -> Result<()> {
        let flashloan = &mut ctx.accounts.flashloan;

        flashloan.token_authority_bump = *ctx.bumps.get("token_authority").unwrap();
        flashloan.authority = ctx.accounts.authority.key();
        // TODO: Add check that fee less than 10%
        flashloan.fee = Fee::from_basis_points(basis_points);

        Ok(())
    }

    // TODO: Add instruction for fee setup

    /// Add pool for a given token mint, setup a pool, token account and lp token mint
    pub fn add_pool(ctx: Context<AddPool>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        pool.bump = *ctx.bumps.get("pool").unwrap();
        pool.borrowing = false;
        pool.token_mint = ctx.accounts.token_mint.key();
        pool.pool_token = ctx.accounts.pool_token.key();
        pool.lp_token_mint = ctx.accounts.lp_token_mint.key();
        // TODO: independent fee setup for pools
        pool.fee = ctx.accounts.flashloan.fee;

        Ok(())
    }

    /// Receive tokens and mint lp tokens
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.pool.borrowing, FlashLoanError::Borrowing);

        // we need to compute how many tokens return for LP-shares
        let lp_supply = ctx.accounts.lp_token_mint.supply;
        let token_supply = ctx.accounts.pool_token.amount;
        let shares_for_user = shares_from_value(
            amount,
            token_supply,
            lp_supply,
        )?;

        let key = ctx.accounts.flashloan.key();
        let seeds = &[
            key.as_ref(), FLASHLOAN_NAMESPACE.as_ref(),
            &[ctx.accounts.flashloan.token_authority_bump],
        ];
        let singer_seeds = &[&seeds[..]];

        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_token.to_account_info(),
                to: ctx.accounts.pool_token.to_account_info(),
                authority: ctx.accounts.token_authority.to_account_info(),
            },
            singer_seeds,
        );

        token::transfer(transfer_ctx, amount)?;

        let mint_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.lp_token_mint.to_account_info(),
                to: ctx.accounts.user_lp_token.to_account_info(),
                authority: ctx.accounts.token_authority.to_account_info(),
            },
            singer_seeds,
        );

        token::mint_to(mint_ctx, shares_for_user)?;

        emit!(DepositEvent {
            token_mint: ctx.accounts.pool.token_mint,
            token_amount: amount,
            lp_amount: shares_for_user,
        });

        Ok(())
    }

    /// Burn lp and pay out tokens
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.pool.borrowing, FlashLoanError::Borrowing);

        let lp_supply = ctx.accounts.lp_token_mint.supply;
        let token_supply = ctx.accounts.pool_token.amount;
        let tokens_for_user = value_from_shares(
            amount,
            token_supply,
            lp_supply,
        )?;

        let key = ctx.accounts.flashloan.key();
        let seeds = &[
            key.as_ref(), FLASHLOAN_NAMESPACE.as_ref(),
            &[ctx.accounts.flashloan.token_authority_bump],
        ];
        let singer_seeds = &[&seeds[..]];

        let burn_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.lp_token_mint.to_account_info(),
                to: ctx.accounts.user_lp_token.to_account_info(),
                authority: ctx.accounts.token_authority.to_account_info(),
            },
            singer_seeds,
        );

        token::burn(burn_ctx, amount)?;

        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.pool_token.to_account_info(),
                to: ctx.accounts.user_token.to_account_info(),
                authority: ctx.accounts.token_authority.to_account_info(),
            },
            singer_seeds,
        );

        token::transfer(transfer_ctx, tokens_for_user)?;

        emit!(WithdrawEvent {
            token_mint: ctx.accounts.pool.token_mint,
            token_amount: tokens_for_user,
            lp_amount: amount
        });

        Ok(())
    }

    pub fn mint_voucher(ctx: Context<MintVoucher>) -> Result<()> {
        Ok(())
    }

    // Confirms there exists a matching repay, then lends tokens
    pub fn borrow(ctx: Context<Borrow>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.pool.borrowing, FlashLoanError::Borrowing);

        let ixns = ctx.accounts.instructions.to_account_info();

        // make sure this isn't a cpi call
        let current_idx = instructions::load_current_index_checked(&ixns)? as usize;
        let current_ixn = instructions::load_instruction_at_checked(current_idx, &ixns)?;
        require!(current_ixn.program_id == *ctx.program_id, FlashLoanError::CpiBorrow);

        // loop through instructions, looking for an equivalent repay to this borrow
        let mut idx = current_idx + 1;
        let expected_sighash = u64::from_be_bytes(Repay::SIGHASH[..8].try_into().unwrap());
        let expected_repay =
            amount
            .checked_add(ctx.accounts.pool.fee.apply(amount))
                .ok_or_else(|| error!(FlashLoanError::CalculationFailure))?;

        loop {
            // get the next instruction, die if theres no more
            if let Ok(ixn) = instructions::load_instruction_at_checked(idx, &ixns) {
                let actual_sighash = u64::from_be_bytes(ixn.data[..8].try_into().unwrap());

                // check if we have a toplevel repay toward the same pool
                // if so, confirm the amount, otherwise next instruction
                if ixn.program_id == *ctx.program_id
                    && actual_sighash == expected_sighash
                    && ixn.accounts[2].pubkey == ctx.accounts.pool.key() {
                    if u64::from_le_bytes(ixn.data[8..16].try_into().unwrap()) == expected_repay {
                        break;
                    } else {
                        return Err(error!(FlashLoanError::IncorrectRepay));
                    }
                } else {
                    idx += 1;
                }
            } else {
                return Err(error!(FlashLoanError::NoRepay));
            }
        }

        let key = ctx.accounts.flashloan.key();
        let seeds = &[
            key.as_ref(), FLASHLOAN_NAMESPACE.as_ref(),
            &[ctx.accounts.flashloan.token_authority_bump],
        ];
        let singer_seeds = &[&seeds[..]];

        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.pool_token.to_account_info(),
                to: ctx.accounts.user_token.to_account_info(),
                authority: ctx.accounts.token_authority.to_account_info(),
            },
            singer_seeds,
        );

        token::transfer(transfer_ctx, amount)?;
        ctx.accounts.pool.borrowing = true;

        emit!(BorrowEvent{
            token_mint: ctx.accounts.pool.token_mint,
            amount,
        });

        Ok(())
    }

    pub fn repay(ctx: Context<Repay>, amount: u64) -> Result<()> {
        let ixns = ctx.accounts.instructions.to_account_info();

        // make sure this isn't a cpi call
        let current_idx = instructions::load_current_index_checked(&ixns)? as usize;
        let current_ixn = instructions::load_instruction_at_checked(current_idx, &ixns)?;
        require!(current_ixn.program_id == *ctx.program_id, FlashLoanError::CpiBorrow);

        let key = ctx.accounts.flashloan.key();
        let seeds = &[
            key.as_ref(), FLASHLOAN_NAMESPACE.as_ref(),
            &[ctx.accounts.flashloan.token_authority_bump],
        ];
        let singer_seeds = &[&seeds[..]];

        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_token.to_account_info(),
                to: ctx.accounts.pool_token.to_account_info(),
                authority: ctx.accounts.token_authority.to_account_info(),
            },
            singer_seeds,
        );

        token::transfer(transfer_ctx, amount)?;
        ctx.accounts.pool.borrowing = false;

        emit!(RepayEvent{
            token_mint: ctx.accounts.pool.token_mint,
            amount,
        });

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
pub struct Deposit<'info> {
    pub flashloan: Account<'info, FlashLoan>,

    #[account(
        seeds = [flashloan.key().as_ref(), FLASHLOAN_NAMESPACE.as_ref()],
        bump = flashloan.token_authority_bump
    )]
    /// CHECK: Checked above, used only for bump calc
    pub token_authority: UncheckedAccount<'info>,

    #[account(
        seeds = [flashloan.key().as_ref(), pool.token_mint.as_ref()],
        bump = pool.bump
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [flashloan.key().as_ref(), TOKEN_NAMESPACE.as_ref(), pool.token_mint.as_ref()],
        bump
    )]
    pub pool_token: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [flashloan.key().as_ref(), LP_TOKEN_NAMESPACE.as_ref(), pool.token_mint.as_ref()],
        bump
    )]
    pub lp_token_mint: Account<'info, Mint>,

    #[account(mut, constraint =  user_token.mint == pool.token_mint)]
    pub user_token: Account<'info, TokenAccount>,

    #[account(mut, constraint = user_lp_token.mint == pool.lp_token_mint)]
    pub user_lp_token: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    pub flashloan: Account<'info, FlashLoan>,

    #[account(
        seeds = [flashloan.key().as_ref(), FLASHLOAN_NAMESPACE.as_ref()],
        bump = flashloan.token_authority_bump
    )]
    /// CHECK: Checked above, used only for bump calc
    pub token_authority: UncheckedAccount<'info>,

    #[account(
        seeds = [flashloan.key().as_ref(), pool.token_mint.as_ref()],
        bump = pool.bump
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [flashloan.key().as_ref(), TOKEN_NAMESPACE.as_ref(), pool.token_mint.as_ref()],
        bump
    )]
    pub pool_token: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [flashloan.key().as_ref(), LP_TOKEN_NAMESPACE.as_ref(), pool.token_mint.as_ref()],
        bump
    )]
    pub lp_token_mint: Account<'info, Mint>,

    #[account(mut, constraint =  user_token.mint == pool.token_mint)]
    pub user_token: Account<'info, TokenAccount>,

    #[account(mut, constraint = user_lp_token.mint == pool.lp_token_mint)]
    pub user_lp_token: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct MintVoucher {}

#[derive(Accounts)]
pub struct Borrow<'info> {
    pub flashloan: Account<'info, FlashLoan>,

    #[account(
        seeds = [flashloan.key().as_ref(), FLASHLOAN_NAMESPACE.as_ref()],
        bump = flashloan.token_authority_bump
    )]
    /// CHECK: Checked above, used only for bump calc
    pub token_authority: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [flashloan.key().as_ref(), pool.token_mint.as_ref()],
        bump = pool.bump
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [flashloan.key().as_ref(), TOKEN_NAMESPACE.as_ref(), pool.token_mint.as_ref()],
        bump
    )]
    pub pool_token: Account<'info, TokenAccount>,

    #[account(mut, constraint =  user_token.mint == pool.token_mint)]
    pub user_token: Account<'info, TokenAccount>,

    #[account(address = instructions::ID)]
    /// CHECK: Checked above, sysvar::instructions
    pub instructions: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Repay<'info> {
    pub flashloan: Account<'info, FlashLoan>,

    #[account(
        seeds = [flashloan.key().as_ref(), FLASHLOAN_NAMESPACE.as_ref()],
        bump = flashloan.token_authority_bump
    )]
    /// CHECK: Checked above, used only for bump calc
    pub token_authority: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [flashloan.key().as_ref(), pool.token_mint.as_ref()],
        bump = pool.bump
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [flashloan.key().as_ref(), TOKEN_NAMESPACE.as_ref(), pool.token_mint.as_ref()],
        bump
    )]
    pub pool_token: Account<'info, TokenAccount>,

    #[account(mut, constraint =  user_token.mint == pool.token_mint)]
    pub user_token: Account<'info, TokenAccount>,

    #[account(address = instructions::ID)]
    /// CHECK: Checked above, sysvar::instructions
    pub instructions: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}

impl Repay<'_> {
    // https://github.com/project-serum/anchor/blob/9e070870f4815849e99f19700d675638d3443b8f/lang/syn/src/codegen/program/dispatch.rs#L119
    //
    // Sha256("global:<rust-identifier>")[..8],
    const SIGHASH: [u8; 32] = Sha256::new()
        .update(b"global:repay")
        .finalize();
}

#[account]
pub struct FlashLoan {
    pub token_authority_bump: u8,
    pub authority: Pubkey,
    pub fee: Fee,
}

impl FlashLoan {
    const LEN: usize = 8 + 1 + 32 + 4;
}

#[account]
pub struct Pool {
    pub bump: u8,
    pub borrowing: bool,
    pub fee: Fee,
    pub token_mint: Pubkey,
    pub pool_token: Pubkey,
    pub lp_token_mint: Pubkey,
}

impl Pool {
    const LEN: usize = 8 + 2 + 32*3 + 4;
}

#[derive(
    Clone, Copy, Debug, Default, AnchorSerialize, AnchorDeserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct Fee {
    pub basis_points: u32,
}

impl Fee {
    pub fn from_basis_points(basis_points: u32) -> Self {
        Self { basis_points }
    }

    pub fn apply(&self, amount: u64) -> u64 {
        // LMT no error possible
        (amount as u128 * self.basis_points as u128 / 10_000_u128) as u64
    }
}

// -----------------------------------------------------------------------------------------------

#[event]
pub struct DepositEvent {
    pub token_mint: Pubkey,
    pub token_amount: u64,
    pub lp_amount: u64,
}

#[event]
pub struct WithdrawEvent {
    pub token_mint: Pubkey,
    pub token_amount: u64,
    pub lp_amount: u64,
}

#[event]
pub struct BorrowEvent {
    pub token_mint: Pubkey,
    pub amount: u64,
}

#[event]
pub struct RepayEvent {
    pub token_mint: Pubkey,
    pub amount: u64,
}

#[error_code]
pub enum FlashLoanError {
    NoRepay,
    IncorrectRepay,
    CpiBorrow,
    CpiRepay,
    Borrowing,
    CalculationFailure,
}