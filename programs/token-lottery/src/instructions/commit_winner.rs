use anchor_lang::prelude::*;
use switchboard_on_demand::{randomness, RandomnessAccountData};

use crate::{error::ErrorCode, TokenLottery};


#[derive(Accounts)]
pub struct RevealWinner<'info>{
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds=[b"token_lottery".as_ref()],
        bump=token_lottery.bump
    )]
    pub token_lottery: Account<'info, TokenLottery>,

    ///CHECK: This Account is checked by switchboard program
    pub randomness_account: UncheckedAccount<'info>
}


pub fn process_commit_reveal(ctx: Context<RevealWinner>)->Result<()>{
 
     
        let clock = Clock::get()?;
        let token_lottery = &mut ctx.accounts.token_lottery;
        if ctx.accounts.payer.key() != token_lottery.authority {
            return Err(ErrorCode::NotAuthorized.into());
        }

        let randomness_data = RandomnessAccountData::parse(ctx.accounts.randomness_account.data.borrow()).unwrap();

        if randomness_data.seed_slot != clock.slot - 1 {
            return Err(ErrorCode::RandomnessAlreadyRevealed.into());
        }

        token_lottery.randomness_account = ctx.accounts.randomness_account.key();

 Ok(())
}