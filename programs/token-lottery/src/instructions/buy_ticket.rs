use anchor_lang::{prelude::*, system_program};
use anchor_spl::{associated_token::AssociatedToken, metadata::{create_master_edition_v3, create_metadata_accounts_v3, mpl_token_metadata::types::{CollectionDetails, Creator, DataV2}, set_and_verify_sized_collection_item, CreateMasterEditionV3, CreateMetadataAccountsV3, Metadata, SetAndVerifySizedCollectionItem}, token_interface::{mint_to, Mint, MintTo, TokenAccount, TokenInterface}};

use crate::{error::ErrorCode, symbol, url, TokenLottery, NAME};

#[derive(Accounts)]
pub struct BuyTicket<'info>{
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds=[b"token_lottery".as_ref()],
        bump=token_lottery.bump
    )]
    pub token_lottery: Account<'info, TokenLottery>,

    #[account(
        init,
        payer=payer,
        seeds=[token_lottery.total_tickets.to_le_bytes().as_ref()],
        bump,
        mint::decimals=0,
        mint::authority=collection_mint,
        mint::freeze_authority=collection_mint,
        mint::token_program=token_program
    )]
    pub ticket_mint: InterfaceAccount<'info, Mint>,

     #[account(
            mut, 
            seeds=[b"metadata",
            token_metadata_program.key().as_ref(), ticket_mint.key().as_ref()], bump,
            seeds::program=token_metadata_program.key()
        )]
    ///CHECK: These are checked by the token metadata program
    pub ticket_metadata: UncheckedAccount<'info>,

     #[account(
            mut, 
            seeds=[b"metadata",
            token_metadata_program.key().as_ref(), collection_mint.key().as_ref()], bump,
            seeds::program=token_metadata_program.key()
        )]
    ///CHECK: These are checked by the token metadata program
    pub collection_metadata: UncheckedAccount<'info>,

     #[account(
        mut, 
        seeds=[b"metadata", token_metadata_program.key().as_ref(), ticket_mint.key().as_ref(), b"edition"], 
        bump, 
        seeds::program=token_metadata_program
    )]
    ///CHECK: These are checked by the token metadata program
    pub ticket_master_edition: UncheckedAccount<'info>,

     #[account(
        mut, 
        seeds=[b"metadata", token_metadata_program.key().as_ref(), collection_mint.key().as_ref(), b"edition"], 
        bump, 
        seeds::program=token_metadata_program
    )]
    ///CHECK: These are checked by the token metadata program
    pub collection_master_edition: UncheckedAccount<'info>,

     pub token_metadata_program: Program<'info, Metadata>,
     #[account(
        init,
        payer=payer,
        associated_token::mint=ticket_mint,
        associated_token::authority=payer,
        associated_token::token_program=token_program
     )]
     pub destination: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        seeds=[b"collection_mint".as_ref()] ,
        bump
    )]
    pub collection_mint: InterfaceAccount<'info, Mint>,

     #[account(
        mut,
        seeds = [b"metadata", token_metadata_program.key().as_ref(), 
        ticket_mint.key().as_ref()],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    /// CHECK: This account will be initialized by the metaplex program
    pub metadata: UncheckedAccount<'info>,

     #[account(
        mut,
        seeds = [b"metadata", token_metadata_program.key().as_ref(), 
            ticket_mint.key().as_ref(), b"edition"],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    /// CHECK: This account will be initialized by the metaplex program
    pub master_edition: UncheckedAccount<'info>,


    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>
}



pub fn process_buy_ticket(ctx: Context<BuyTicket>)->Result<()>{

    let clock = Clock::get()?;
    let ticket_name=NAME.to_owned()+ctx.accounts.token_lottery.total_tickets.to_string().as_str();

    if clock.slot < ctx.accounts.token_lottery.start_time || clock.slot >ctx.accounts.token_lottery.end_time {
        return Err(ErrorCode::LotteryNotOpen.into())
    }

    //Transfer tokens to the vault

    system_program::transfer(CpiContext::new(ctx.accounts.system_program.to_account_info(), system_program::Transfer { from: ctx.accounts.payer.to_account_info(), to: ctx.accounts.token_lottery.to_account_info() }), ctx.accounts.token_lottery.ticket_price)?;

    let signer_seeds: &[&[&[u8]]] = &[&[
        b"collection_mint".as_ref(),
        &[ctx.bumps.collection_mint]
    ]];

    mint_to(CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), MintTo{
        mint: ctx.accounts.ticket_mint.to_account_info(),
        to: ctx.accounts.destination.to_account_info(),
        authority: ctx.accounts.collection_mint.to_account_info(),
    }, &signer_seeds),
1)?;


  msg!("Creating Metadata Account v3");

    create_metadata_accounts_v3(CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(),
     CreateMetadataAccountsV3 { metadata: ctx.accounts.ticket_metadata.to_account_info(),
        mint: ctx.accounts.ticket_mint.to_account_info(),
        mint_authority: ctx.accounts.collection_mint.to_account_info(),
        payer: ctx.accounts.payer.to_account_info(),
        update_authority: ctx.accounts.collection_mint.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(), 
        rent: ctx.accounts.rent.to_account_info()
     }, 
     signer_seeds
    ), DataV2{
        name: ticket_name.to_string(),
        symbol: symbol.to_string(),
        uri: url.to_string(),
        seller_fee_basis_points: 0,
        creators: None,
        collection: None, 
        uses: None,
    }, true, true, None);

   
    msg!("Creating Master Edition Account");

    create_master_edition_v3(CpiContext::new_with_signer(ctx.accounts.token_metadata_program.to_account_info(),
    CreateMasterEditionV3 { 
        edition: ctx.accounts.ticket_master_edition.to_account_info(),
        mint: ctx.accounts.ticket_mint.to_account_info(),
        update_authority: ctx.accounts.collection_mint.to_account_info(),
        mint_authority: ctx.accounts.collection_mint.to_account_info(),
        payer: ctx.accounts.payer.to_account_info(),
        metadata: ctx.accounts.ticket_metadata.to_account_info(),
        token_program: ctx.accounts.token_program.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
        rent: ctx.accounts.rent.to_account_info() }, signer_seeds), Some(0));


        msg!("Setting and verifying collection");

        set_and_verify_sized_collection_item(CpiContext::new_with_signer(ctx.accounts.token_metadata_program.to_account_info(),
            SetAndVerifySizedCollectionItem{
                metadata: ctx.accounts.metadata.to_account_info(),
                collection_authority: ctx.accounts.collection_mint.to_account_info(),
                payer: ctx.accounts.payer.to_account_info(),
                update_authority: ctx.accounts.collection_mint.to_account_info(),
                collection_metadata: ctx.accounts.collection_metadata.to_account_info(),
                collection_master_edition: ctx.accounts.collection_master_edition.to_account_info(),
                collection_mint: ctx.accounts.collection_mint.to_account_info()
            }, signer_seeds),
            None)?;

            ctx.accounts.token_lottery.total_tickets += 1;

    Ok(())
} 