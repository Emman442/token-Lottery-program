use anchor_lang::prelude::*;

use crate::{error::ErrorCode, TokenLottery};
use switchboard_on_demand::RandomnessAccountData;

#[derive(Accounts)]
pub struct CommitRandomness<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(mut, seeds =[ b"token_lottery".as_ref()], bump = token_lottery.bump)]
    pub token_lottery: Account<'info, TokenLottery>,

    ///CHECK: This is checeked by switchboard randomness account
    pub randomness_account: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn process_commit_randomness(ctx: Context<CommitRandomness>) -> Result<()> {
    let clock = Clock::get()?;

    let token_lottery = &mut ctx.accounts.token_lottery;

    if ctx.accounts.payer.key() != token_lottery.authority {
        return Err(ErrorCode::NotAuthorized.into());
    }

    let randomness_data =
        RandomnessAccountData::parse(ctx.accounts.randomness_account.data.borrow()).unwrap();

        if randomness_data.seed_slot !=clock.slot-1{
            return Err(ErrorCode::RandomnessAlreadyRevealed.into());
        }

        token_lottery.randomness_account = ctx.accounts.randomness_account.key();


    Ok(())
}
