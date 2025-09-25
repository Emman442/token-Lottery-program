use anchor_lang::prelude::*;
use switchboard_on_demand::RandomnessAccountData;

use crate::{error::ErrorCode, TokenLottery};

#[derive(Accounts)]
pub struct ChooseWinner<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"token_lottery".as_ref()],
        bump = token_lottery.bump,
    )]
    pub token_lottery: Account<'info, TokenLottery>,

    /// CHECK: The account's data is validated manually within the handler.
    pub randomness_account_data: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn process_choose_winner(ctx: Context<ChooseWinner>) -> Result<()> {
        let clock = Clock::get()?;
        let token_lottery = &mut ctx.accounts.token_lottery;

        if ctx.accounts.randomness_account_data.key() != token_lottery.randomness_account {
            return Err(ErrorCode::IncorrectRandomnessAccount.into());
        }
        if ctx.accounts.payer.key() != token_lottery.authority {
            return Err(ErrorCode::NotAuthorized.into());
        }
        if clock.slot < token_lottery.end_time {
            msg!("Current slot: {}", clock.slot);
            msg!("End slot: {}", token_lottery.end_time);
            return Err(ErrorCode::LotteryNotCompleted.into());
        }
        require!(token_lottery.winner_chosen == false, ErrorCode::WinnerChosen);

        let randomness_data = 
            RandomnessAccountData::parse(ctx.accounts.randomness_account_data.data.borrow()).unwrap();
        let revealed_random_value = randomness_data.get_value(clock.slot)
            .map_err(|_| ErrorCode::RandomnessNotResolved)?;

        msg!("Randomness result: {}", revealed_random_value[0]);
        msg!("Ticket num: {}", token_lottery.total_tickets);

        let randomness_result = 
            revealed_random_value[0] as u64 % token_lottery.total_tickets;

        msg!("Winner: {}", randomness_result);

        token_lottery.winner = randomness_result;
        token_lottery.winner_chosen = true;

        Ok(())
    }