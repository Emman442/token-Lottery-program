pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("8Lxsrsym8unoU34GDRyiENuZqMKdqTA4jAuYRSz13yGb");

#[program]
pub mod token_lottery {
    use super::*;

    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        start_time: u64,
        end_time: u64,
        prize: u64,
    ) -> Result<()> {
        process_initialize_config(ctx, start_time, end_time, prize)
    }
    pub fn initialize_lottery(ctx: Context<InitializeLottery>) -> Result<()> {
        process_initialize_lottery(ctx)
    }

    pub fn buy_ticket(ctx: Context<BuyTicket>) -> Result<()> {
        process_buy_ticket(ctx)
    }


    pub fn commit_randomness(ctx: Context<CommitRandomness>) -> Result<()> {
        process_commit_randomness(ctx)
    }

    
    pub fn commit_a_winner(ctx: Context<RevealWinner>) -> Result<()> {
        process_commit_reveal(ctx)
    }

    pub fn choose_winner(ctx: Context<ChooseWinner>)->Result<()>{
      process_choose_winner(ctx)
    }

    pub fn claim_winnings(ctx: Context<ClaimWinnings>)->Result<()>{
      process_claim_winnings(ctx)
    }
}
