use anchor_lang::prelude::*;

use crate::TokenLottery;


#[derive(Accounts)]
pub struct InitializeConfig<'info>{
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        init, 
        payer=signer,
        space=8+ TokenLottery::INIT_SPACE,
        seeds=[b"token_lottery".as_ref()],
        bump
    )]
    pub token_lottery: Account<'info, TokenLottery>,


    pub system_program: Program<'info, System>
}



pub fn process_initialize_config(ctx: Context<InitializeConfig>, start_time: u64, end_time: u64, price: u64)->Result<()>{
    ctx.accounts.token_lottery.bump=ctx.bumps.token_lottery;
    ctx.accounts.token_lottery.start_time= start_time;
    ctx.accounts.token_lottery.end_time = end_time;
    ctx.accounts.token_lottery.ticket_price=price; 
    ctx.accounts.token_lottery.authority = ctx.accounts.signer.key();
    ctx.accounts.token_lottery.pot_amount = 0;
    ctx.accounts.token_lottery.randomness_account = Pubkey::default();
    ctx.accounts.token_lottery.winner_chosen = false;
Ok(())
}