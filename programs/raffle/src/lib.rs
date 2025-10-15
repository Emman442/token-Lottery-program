use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hash;
use anchor_lang::system_program;
use anchor_spl::metadata::MetadataAccount;
use anchor_spl::metadata::{sign_metadata, SignMetadata};
use anchor_spl::token_interface::{
    mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
};
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::{
        create_master_edition_v3, create_metadata_accounts_v3,
        mpl_token_metadata::types::{CollectionDetails, Creator, DataV2},
        set_and_verify_sized_collection_item, CreateMasterEditionV3, CreateMetadataAccountsV3,
        Metadata, SetAndVerifySizedCollectionItem,
    },
};
use ephemeral_vrf_sdk::anchor::vrf;
use ephemeral_vrf_sdk::instructions::{create_request_randomness_ix, RequestRandomnessParams};
use ephemeral_vrf_sdk::types::SerializableAccountMeta;

#[constant]
pub const SEED: &str = "anchor";

#[constant]
pub const NAME: &str = "Token Lottery Ticket #";

#[constant]
pub const symbol: &str = "TLT";

#[constant]
pub const url: &str =
    "https://raw.githubusercontent.com/Emman442/Quiz-application-with-leaderboard-feature/main/mpl.json";

declare_id!("BQuBEeVWhtjKUSkmGPEoUo5s3zPnukrFQaFE9FTgFCdN");

#[program]
pub mod raffle {
    use super::*;

    pub fn buy_ticket(ctx: Context<BuyTicket>) -> Result<()> {
        let clock = Clock::get()?;
        let ticket_name = NAME.to_owned()
            + ctx
                .accounts
                .token_lottery
                .total_tickets
                .to_string()
                .as_str();

        if clock.unix_timestamp < ctx.accounts.token_lottery.start_time
            || clock.unix_timestamp > ctx.accounts.token_lottery.end_time
        {
            return Err(ErrorCode::LotteryNotOpen.into());
        }

        // Transfer tokens to the vault
        let decimals = ctx.accounts.token_mint.decimals;

        let cpi_accounts = TransferChecked {
            mint: ctx.accounts.token_mint.to_account_info(),
            from: ctx.accounts.payer_token_account.to_account_info(),
            to: ctx.accounts.raffle_vault_account.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
        transfer_checked(
            cpi_context,
            ctx.accounts.token_lottery.ticket_price,
            decimals,
        )?;

        ctx.accounts.token_lottery.pot_amount = ctx
            .accounts
            .token_lottery
            .pot_amount
            .checked_add(ctx.accounts.token_lottery.ticket_price)
            .unwrap();

        let round_id_bytes = ctx.accounts.token_lottery.round_id.to_le_bytes();
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"collection_mint".as_ref(),
            round_id_bytes.as_ref(),
            &[ctx.bumps.collection_mint],
        ]];
        // mint the NFT ticket (1 token) to the destination ATA
        mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.ticket_mint.to_account_info(),
                    to: ctx.accounts.destination.to_account_info(),
                    authority: ctx.accounts.collection_mint.to_account_info(),
                },
                &signer_seeds,
            ),
            1,
        )?;

        msg!("Creating Metadata Account v3");

        create_metadata_accounts_v3(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                CreateMetadataAccountsV3 {
                    metadata: ctx.accounts.ticket_metadata.to_account_info(),
                    mint: ctx.accounts.ticket_mint.to_account_info(),
                    mint_authority: ctx.accounts.collection_mint.to_account_info(),
                    payer: ctx.accounts.payer.to_account_info(),
                    update_authority: ctx.accounts.collection_mint.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
                &signer_seeds,
            ),
            DataV2 {
                name: ticket_name.to_string(),
                symbol: symbol.to_string(),
                uri: url.to_string(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            true,
            true,
            None,
        );

        msg!("Creating Master Edition Account");

        create_master_edition_v3(
            CpiContext::new_with_signer(
                ctx.accounts.token_metadata_program.to_account_info(),
                CreateMasterEditionV3 {
                    edition: ctx.accounts.ticket_master_edition.to_account_info(),
                    mint: ctx.accounts.ticket_mint.to_account_info(),
                    update_authority: ctx.accounts.collection_mint.to_account_info(),
                    mint_authority: ctx.accounts.collection_mint.to_account_info(),
                    payer: ctx.accounts.payer.to_account_info(),
                    metadata: ctx.accounts.ticket_metadata.to_account_info(),
                    token_program: ctx.accounts.token_program.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
                &signer_seeds,
            ),
            Some(0),
        );

        msg!("Setting and verifying collection");

        set_and_verify_sized_collection_item(
            CpiContext::new_with_signer(
                ctx.accounts.token_metadata_program.to_account_info(),
                SetAndVerifySizedCollectionItem {
                    metadata: ctx.accounts.metadata.to_account_info(),
                    collection_authority: ctx.accounts.collection_mint.to_account_info(),
                    payer: ctx.accounts.payer.to_account_info(),
                    update_authority: ctx.accounts.collection_mint.to_account_info(),
                    collection_metadata: ctx.accounts.collection_metadata.to_account_info(),
                    collection_master_edition: ctx
                        .accounts
                        .collection_master_edition
                        .to_account_info(),
                    collection_mint: ctx.accounts.collection_mint.to_account_info(),
                },
                &signer_seeds,
            ),
            None,
        )?;

        // increment the ticket counter (this increases ticket index for next mint)
        ctx.accounts.token_lottery.total_tickets = ctx
            .accounts
            .token_lottery
            .total_tickets
            .checked_add(1)
            .unwrap();

        emit!(BoughtTicket {
            price: ctx.accounts.token_lottery.ticket_price,
            current_total_tickets: ctx.accounts.token_lottery.total_tickets
        });

        Ok(())
    }

    pub fn restart_lottery(
        ctx: Context<RestartLottery>,
        new_start_time: i64,
        new_end_time: i64,
        new_ticket_price: u64,
    ) -> Result<()> {
        let lottery = &mut ctx.accounts.token_lottery;
        lottery.start_time = new_start_time;
        lottery.end_time = new_end_time;
        lottery.ticket_price = new_ticket_price;
        lottery.total_tickets = 0;
        lottery.winner_chosen = false;
        lottery.winner = 0;
        lottery.pot_amount = 0;
        // bump round id to create fresh PDAs for next initialize_lottery
        lottery.round_id = lottery.round_id.checked_add(1).unwrap();
        Ok(())
    }

    pub fn claim_winnings(ctx: Context<ClaimWinnings>) -> Result<()> {
        // Check if winner has been chosen
        msg!(
            "Winner chosen: {}",
            ctx.accounts.token_lottery.winner_chosen
        );
        require!(
            ctx.accounts.token_lottery.winner_chosen,
            ErrorCode::WinnerNotChosen
        );

        // Check if token is a part of the collection
        require!(
            ctx.accounts.metadata.collection.as_ref().unwrap().verified,
            ErrorCode::NotVerifiedTicket
        );
        require!(
            ctx.accounts.metadata.collection.as_ref().unwrap().key
                == ctx.accounts.collection_mint.key(),
            ErrorCode::IncorrectTicket
        );

        let ticket_name = NAME.to_owned() + &ctx.accounts.token_lottery.winner.to_string();
        let metadata_name = ctx.accounts.metadata.name.replace("\u{0}", "");

        msg!("Ticket name: {}", ticket_name);
        msg!("Metdata name: {}", metadata_name);

        // Check if the winner has the winning ticket
        require!(metadata_name == ticket_name, ErrorCode::IncorrectTicket);
        require!(
            ctx.accounts.destination.amount > 0,
            ErrorCode::IncorrectTicket
        );

        // token_lottery is signer authority for reward_vault
        let seeds = &[b"token_lottery".as_ref(), &[ctx.bumps.token_lottery]];
        let signer = &[&seeds[..]];

        let decimals = ctx.accounts.reward_mint.decimals;

        let cpi_accounts = TransferChecked {
            from: ctx.accounts.reward_vault.to_account_info(),
            to: ctx.accounts.winner_token_account.to_account_info(),
            mint: ctx.accounts.reward_mint.to_account_info(),
            authority: ctx.accounts.token_lottery.to_account_info(),
        };

        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);

        transfer_checked(cpi_ctx, ctx.accounts.token_lottery.pot_amount, decimals)?;
        ctx.accounts.token_lottery.pot_amount = 0;

        emit!(WinningsClaimed {
            ticket_name: ticket_name,
            destination_account: ctx.accounts.destination.key()
        });

        Ok(())
    }

    pub fn commit_winner(ctx: Context<CommitWinner>, client_seed: u8) -> Result<()> {
        let clock = Clock::get()?;
        let token_lottery = &mut ctx.accounts.token_lottery;
        if ctx.accounts.payer.key() != token_lottery.authority {
            return Err(ErrorCode::NotAuthorized.into());
        }

        require!(
            clock.unix_timestamp >= token_lottery.end_time,
            ErrorCode::LotteryNotCompleted
        );
        require!(!token_lottery.winner_chosen, ErrorCode::WinnerChosen);

        let ix = create_request_randomness_ix(RequestRandomnessParams {
            payer: ctx.accounts.payer.key(),
            oracle_queue: ctx.accounts.oracle_queue.key(),
            callback_program_id: ID,
            callback_discriminator: instruction::CallbackChooseWinner::DISCRIMINATOR.to_vec(),
            caller_seed: [client_seed; 32],
            // specify token_lottery for callback
            accounts_metas: Some(vec![SerializableAccountMeta {
                pubkey: ctx.accounts.token_lottery.key(),
                is_signer: false,
                is_writable: true,
            }]),
            ..Default::default()
        });
        ctx.accounts
            .invoke_signed_vrf(&ctx.accounts.payer.to_account_info(), &ix)?;

        emit!(WinnerCommited {
            oracle_queue: ctx.accounts.oracle_queue.key()
        });
        Ok(())
    }

    pub fn callback_choose_winner(
        ctx: Context<CallbackChooseWinnerCtx>,
        randomness: [u8; 32],
    ) -> Result<()> {
        msg!("🎲 Callback invoked with randomness!");
        let clock = Clock::get()?;

        let token_lottery = &mut ctx.accounts.token_lottery;

        require!(
            token_lottery.winner_chosen == false,
            ErrorCode::WinnerChosen
        );

        require!(
            token_lottery.total_tickets > 0,
            ErrorCode::LotteryNotCompleted
        );

        let random_number = ephemeral_vrf_sdk::rnd::random_u8_with_range(
            &randomness,
            0,
            token_lottery.total_tickets as u8 - 1,
        );
        let winner_index = random_number as u64;
        token_lottery.winner = winner_index;
        token_lottery.winner_chosen = true;
        emit!(SelectWinner {
            winner: ctx.accounts.token_lottery.winner,
            winner_chosen: ctx.accounts.token_lottery.winner_chosen
        });

        Ok(())
    }

    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        start_time: i64,
        end_time: i64,
        price: u64,
    ) -> Result<()> {
        ctx.accounts.token_lottery.bump = ctx.bumps.token_lottery;
        ctx.accounts.token_lottery.start_time = start_time;
        ctx.accounts.token_lottery.end_time = end_time;
        ctx.accounts.token_lottery.ticket_price = price;
        ctx.accounts.token_lottery.authority = ctx.accounts.signer.key();
        ctx.accounts.token_lottery.pot_amount = 0;
        ctx.accounts.token_lottery.winner_chosen = false;
        // initial round id 0
        ctx.accounts.token_lottery.round_id = 0;
        ctx.accounts.token_lottery.total_tickets = 0;
        ctx.accounts.token_lottery.winner = 0;

        emit!(InitializedConfig {
            start_time: start_time,
            end_time: end_time,
            price: price,
        });
        Ok(())
    }

    pub fn initialize_lottery(ctx: Context<InitializeLottery>) -> Result<()> {
        // Store round_id bytes in a variable so they live long enough
        let round_id_bytes = ctx.accounts.token_lottery.round_id.to_le_bytes();

        // signer seeds for collection_mint PDA
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"collection_mint".as_ref(),
            round_id_bytes.as_ref(),
            &[ctx.bumps.collection_mint],
        ]];
        msg!("Creating Mint Account");
        // Mint 1 token of the collection (collection supply/marker)
        mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.collection_mint.to_account_info(),
                    to: ctx.accounts.collection_token_account.to_account_info(),
                    authority: ctx.accounts.collection_mint.to_account_info(),
                },
                &signer_seeds,
            ),
            1,
        )?;

        msg!("Creating Metadata Account v3");
        create_metadata_accounts_v3(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                CreateMetadataAccountsV3 {
                    metadata: ctx.accounts.metadata.to_account_info(),
                    mint: ctx.accounts.collection_mint.to_account_info(),
                    mint_authority: ctx.accounts.collection_mint.to_account_info(),
                    payer: ctx.accounts.payer.to_account_info(),
                    update_authority: ctx.accounts.collection_mint.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
                &signer_seeds,
            ),
            DataV2 {
                name: NAME.to_string(),
                symbol: symbol.to_string(),
                uri: url.to_string(),
                seller_fee_basis_points: 0,
                creators: Some(vec![Creator {
                    address: ctx.accounts.collection_mint.key(),
                    verified: false,
                    share: 100,
                }]),
                collection: None,
                uses: None,
            },
            true,
            true,
            Some(CollectionDetails::V1 { size: 0 }),
        );

        msg!("Creating Master Edition Account");
        create_master_edition_v3(
            CpiContext::new_with_signer(
                ctx.accounts.token_metadata_program.to_account_info(),
                CreateMasterEditionV3 {
                    edition: ctx.accounts.master_edition.to_account_info(),
                    mint: ctx.accounts.collection_mint.to_account_info(),
                    update_authority: ctx.accounts.collection_mint.to_account_info(),
                    mint_authority: ctx.accounts.collection_mint.to_account_info(),
                    payer: ctx.accounts.payer.to_account_info(),
                    metadata: ctx.accounts.metadata.to_account_info(),
                    token_program: ctx.accounts.token_program.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
                &signer_seeds,
            ),
            Some(0),
        );

        msg!("Verifying Collection...");
        sign_metadata(CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            SignMetadata {
                creator: ctx.accounts.collection_mint.to_account_info(),
                metadata: ctx.accounts.metadata.to_account_info(),
            },
            &signer_seeds,
        ));

        emit!(InitializedLottery {
            collection_mint: ctx.accounts.collection_mint.key()
        });
        Ok(())
    }
}

// ---------------------------- Accounts ---------------------------- //

#[derive(Accounts)]
pub struct InitializeLottery<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    // collection_mint is now round-scoped (seed includes token_lottery.round_id)
    #[account(
        init,
        payer = payer,
        mint::decimals = 0,
        mint::authority = collection_mint,
        mint::freeze_authority = collection_mint,
        seeds = [b"collection_mint".as_ref(), token_lottery.round_id.to_le_bytes().as_ref()],
        bump
    )]
    pub collection_mint: InterfaceAccount<'info, Mint>,

    // collection_token_account is the ATA for collection_mint owned by collection_mint PDA
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = collection_mint,
        associated_token::authority = collection_mint,
    )]
    pub collection_token_account: InterfaceAccount<'info, TokenAccount>,

    // metadata PDA for the collection mint (metaplex)
    #[account(
        mut,
        seeds = [b"metadata", token_metadata_program.key().as_ref(), collection_mint.key().as_ref()],
        bump,
        seeds::program = token_metadata_program.key()
    )]
    /// CHECK: checked by metadata program
    pub metadata: UncheckedAccount<'info>,

    // token_lottery config (persistent)
    #[account(
        seeds = [b"token_lottery".as_ref()],
        bump = token_lottery.bump
    )]
    pub token_lottery: Account<'info, TokenLottery>,

    #[account(
        mut, 
        seeds=[b"metadata", token_metadata_program.key().as_ref(), collection_mint.key().as_ref(), b"edition"], 
        bump, 
        seeds::program=token_metadata_program
    )]
    ///CHECK: These are checked by the token metadata program
    pub master_edition: UncheckedAccount<'info>,
    pub token_metadata_program: Program<'info, Metadata>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[vrf]
#[derive(Accounts)]
pub struct CommitWinner<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds=[b"token_lottery".as_ref()],
        bump=token_lottery.bump
    )]
    pub token_lottery: Account<'info, TokenLottery>,

    /// CHECK: The oracle queue
    #[account(mut, address = ephemeral_vrf_sdk::consts::DEFAULT_QUEUE)]
    pub oracle_queue: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
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

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimWinnings<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"token_lottery".as_ref()],
        bump
    )]
    pub token_lottery: Account<'info, TokenLottery>,

    pub reward_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = reward_mint,
        associated_token::authority = token_lottery,
        associated_token::token_program = token_program,
    )]
    pub reward_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub winner_token_account: InterfaceAccount<'info, TokenAccount>,

    // collection_mint must match the round's collection mint (should include round_id)
    #[account(
        mut,
        seeds = [b"collection_mint".as_ref(), token_lottery.round_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub collection_mint: InterfaceAccount<'info, Mint>,

    // ticket mint derived from round_id + ticket index / winner
    #[account(
        seeds = [token_lottery.round_id.to_le_bytes().as_ref(), token_lottery.winner.to_le_bytes().as_ref()],
        bump,
    )]
    pub ticket_mint: InterfaceAccount<'info, Mint>,

    #[account(
        seeds = [b"metadata", token_metadata_program.key().as_ref(), ticket_mint.key().as_ref()],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub metadata: Account<'info, MetadataAccount>,

    #[account(
        associated_token::mint = ticket_mint,
        associated_token::authority = payer,
        associated_token::token_program = token_program,
    )]
    pub destination: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"metadata", token_metadata_program.key().as_ref(), collection_mint.key().as_ref()],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_metadata: Account<'info, MetadataAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub token_metadata_program: Program<'info, Metadata>,
}

#[derive(Accounts)]
pub struct RestartLottery<'info> {
    #[account(
        mut,
        seeds = [b"token_lottery".as_ref()],
        bump = token_lottery.bump,
    )]
    pub token_lottery: Account<'info, TokenLottery>,

    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct CallbackChooseWinnerCtx<'info> {
    /// CHECK: VRF program identity
    #[account(address = ephemeral_vrf_sdk::consts::VRF_PROGRAM_IDENTITY)]
    pub vrf_program_identity: Signer<'info>,

    #[account(mut)]
    pub token_lottery: Account<'info, TokenLottery>,
}

#[derive(Accounts)]
pub struct BuyTicket<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds=[b"token_lottery".as_ref()],
        bump=token_lottery.bump
    )]
    pub token_lottery: Box<Account<'info, TokenLottery>>,

    #[account(
        mut,
        constraint = payer_token_account.mint == token_mint.key(),
        constraint = payer_token_account.owner == payer.key(),
    )]
    pub payer_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    // Vault for reward tokens (init_if_needed so we don't re-init a vault that already exists)
    #[account(
       init_if_needed,
       payer = payer,
       associated_token::mint = token_mint,
       associated_token::authority = token_lottery,
       associated_token::token_program = token_program,
    )]
    pub raffle_vault_account: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_mint: Box<InterfaceAccount<'info, Mint>>,

    // ticket_mint is now round-scoped + ticket index to avoid collisions
    #[account(
        init,
        payer=payer,
        seeds=[token_lottery.round_id.to_le_bytes().as_ref(), token_lottery.total_tickets.to_le_bytes().as_ref()],
        bump,
        mint::decimals=0,
        mint::authority=collection_mint,
        mint::freeze_authority=collection_mint,
        mint::token_program=token_program
    )]
    pub ticket_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut, 
        seeds=[b"metadata", token_metadata_program.key().as_ref(), ticket_mint.key().as_ref()],
        bump,
        seeds::program=token_metadata_program.key()
    )]
    ///CHECK: These are checked by the token metadata program
    pub ticket_metadata: UncheckedAccount<'info>,

    #[account(
        init,
        payer=payer,
        associated_token::mint=ticket_mint,
        associated_token::authority=payer,
        associated_token::token_program=token_program
    )]
    pub destination: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut, 
        seeds=[b"metadata", token_metadata_program.key().as_ref(), collection_mint.key().as_ref()],
        bump,
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

    // collection_mint is round-scoped
    #[account(
        mut,
        seeds=[b"collection_mint".as_ref(), token_lottery.round_id.to_le_bytes().as_ref()],
        bump
    )]
    pub collection_mint: Box<InterfaceAccount<'info, Mint>>,

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
    pub rent: Sysvar<'info, Rent>,
}

// ---------------------------- Errors & Events & State ---------------------------- //

#[error_code]
pub enum ErrorCode {
    #[msg("Lottery is not open yet.")]
    LotteryNotOpen,
    #[msg("You're Not Authorized!.")]
    NotAuthorized,
    #[msg("Randomnes Already Revealed")]
    RandomnessAlreadyRevealed,
    #[msg("Lottery Not Completed")]
    LotteryNotCompleted,
    #[msg("Incorrect Randomness Account")]
    IncorrectRandomnessAccount,
    #[msg("Randomness Not Resolved")]
    RandomnessNotResolved,
    #[msg("Winner Already Chosen")]
    WinnerChosen,
    #[msg("Ticket Is Incorrect")]
    IncorrectTicket,
    #[msg("Winner Not Chosen")]
    WinnerNotChosen,
    #[msg("Ticket Not Verified")]
    NotVerifiedTicket,
}

#[event]
pub struct InitializedConfig {
    pub start_time: i64,
    pub end_time: i64,
    pub price: u64,
}
#[event]
pub struct InitializedLottery {
    pub collection_mint: Pubkey,
}

#[event]
pub struct BoughtTicket {
    pub price: u64,
    pub current_total_tickets: u64,
}
#[event]
pub struct SelectWinner {
    pub winner: u64,
    pub winner_chosen: bool,
}
#[event]
pub struct WinningsClaimed {
    pub ticket_name: String,
    pub destination_account: Pubkey,
}

#[event]
pub struct WinnerCommited {
    pub oracle_queue: Pubkey,
}

#[account]
#[derive(InitSpace)]
pub struct TokenLottery {
    pub round_id: u64,
    pub winner: u64,
    pub winner_chosen: bool,
    pub start_time: i64,
    pub end_time: i64,
    pub pot_amount: u64,
    pub total_tickets: u64,
    pub ticket_price: u64,
    pub authority: Pubkey,
    pub bump: u8,
}

// use anchor_lang::prelude::*;
// use anchor_lang::solana_program::hash::hash;
// use anchor_lang::system_program;
// use anchor_spl::metadata::MetadataAccount;
// use anchor_spl::metadata::{sign_metadata, SignMetadata};
// use anchor_spl::token_interface::{
//     mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
// };
// use anchor_spl::{
//     associated_token::AssociatedToken,
//     metadata::{
//         create_master_edition_v3, create_metadata_accounts_v3,
//         mpl_token_metadata::types::{CollectionDetails, Creator, DataV2},
//         set_and_verify_sized_collection_item, CreateMasterEditionV3, CreateMetadataAccountsV3,
//         Metadata, SetAndVerifySizedCollectionItem,
//     },
// };
// use ephemeral_vrf_sdk::anchor::vrf;
// use ephemeral_vrf_sdk::instructions::{create_request_randomness_ix, RequestRandomnessParams};
// use ephemeral_vrf_sdk::types::SerializableAccountMeta;
// #[constant]
// pub const SEED: &str = "anchor";

// #[constant]
// pub const NAME: &str = "Token Lottery Ticket #";

// #[constant]
// pub const symbol: &str = "TLT";

// #[constant]
// pub const url: &str="https://raw.githubusercontent.com/Emman442/Quiz-application-with-leaderboard-feature/main/mpl.json
// ";
// declare_id!("2mWFhDiwUTJw1BUSSbCQStJzMYeXf4FLA3oaPER6GmfY");
// #[program]
// pub mod raffle {
//     use super::*;

//     pub fn buy_ticket(ctx: Context<BuyTicket>) -> Result<()> {
//         let clock = Clock::get()?;
//         let ticket_name = NAME.to_owned()
//             + ctx
//                 .accounts
//                 .token_lottery
//                 .total_tickets
//                 .to_string()
//                 .as_str();

//         if clock.unix_timestamp < ctx.accounts.token_lottery.start_time
//             || clock.unix_timestamp > ctx.accounts.token_lottery.end_time
//         {
//             return Err(ErrorCode::LotteryNotOpen.into());
//         }

//         //Transfer tokens to the vault

//         let decimals = ctx.accounts.token_mint.decimals;

//         let cpi_accounts = TransferChecked {
//             mint: ctx.accounts.token_mint.to_account_info(),
//             from: ctx.accounts.payer_token_account.to_account_info(),
//             to: ctx.accounts.raffle_vault_account.to_account_info(),
//             authority: ctx.accounts.payer.to_account_info(),
//         };
//         let cpi_program = ctx.accounts.token_program.to_account_info();
//         let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
//         transfer_checked(
//             cpi_context,
//             ctx.accounts.token_lottery.ticket_price,
//             decimals,
//         )?;

//         ctx.accounts.token_lottery.pot_amount = ctx
//             .accounts
//             .token_lottery
//             .pot_amount
//             .checked_add(ctx.accounts.token_lottery.ticket_price)
//             .unwrap();

//         let signer_seeds: &[&[&[u8]]] =
//             &[&[b"collection_mint".as_ref(), &[ctx.bumps.collection_mint]]];

//         mint_to(
//             CpiContext::new_with_signer(
//                 ctx.accounts.token_program.to_account_info(),
//                 MintTo {
//                     mint: ctx.accounts.ticket_mint.to_account_info(),
//                     to: ctx.accounts.destination.to_account_info(),
//                     authority: ctx.accounts.collection_mint.to_account_info(),
//                 },
//                 &signer_seeds,
//             ),
//             1,
//         )?;

//         msg!("Creating Metadata Account v3");

//         create_metadata_accounts_v3(
//             CpiContext::new_with_signer(
//                 ctx.accounts.token_program.to_account_info(),
//                 CreateMetadataAccountsV3 {
//                     metadata: ctx.accounts.ticket_metadata.to_account_info(),
//                     mint: ctx.accounts.ticket_mint.to_account_info(),
//                     mint_authority: ctx.accounts.collection_mint.to_account_info(),
//                     payer: ctx.accounts.payer.to_account_info(),
//                     update_authority: ctx.accounts.collection_mint.to_account_info(),
//                     system_program: ctx.accounts.system_program.to_account_info(),
//                     rent: ctx.accounts.rent.to_account_info(),
//                 },
//                 signer_seeds,
//             ),
//             DataV2 {
//                 name: ticket_name.to_string(),
//                 symbol: symbol.to_string(),
//                 uri: url.to_string(),
//                 seller_fee_basis_points: 0,
//                 creators: None,
//                 collection: None,
//                 uses: None,
//             },
//             true,
//             true,
//             None,
//         );

//         msg!("Creating Master Edition Account");

//         create_master_edition_v3(
//             CpiContext::new_with_signer(
//                 ctx.accounts.token_metadata_program.to_account_info(),
//                 CreateMasterEditionV3 {
//                     edition: ctx.accounts.ticket_master_edition.to_account_info(),
//                     mint: ctx.accounts.ticket_mint.to_account_info(),
//                     update_authority: ctx.accounts.collection_mint.to_account_info(),
//                     mint_authority: ctx.accounts.collection_mint.to_account_info(),
//                     payer: ctx.accounts.payer.to_account_info(),
//                     metadata: ctx.accounts.ticket_metadata.to_account_info(),
//                     token_program: ctx.accounts.token_program.to_account_info(),
//                     system_program: ctx.accounts.system_program.to_account_info(),
//                     rent: ctx.accounts.rent.to_account_info(),
//                 },
//                 signer_seeds,
//             ),
//             Some(0),
//         );

//         msg!("Setting and verifying collection");

//         set_and_verify_sized_collection_item(
//             CpiContext::new_with_signer(
//                 ctx.accounts.token_metadata_program.to_account_info(),
//                 SetAndVerifySizedCollectionItem {
//                     metadata: ctx.accounts.metadata.to_account_info(),
//                     collection_authority: ctx.accounts.collection_mint.to_account_info(),
//                     payer: ctx.accounts.payer.to_account_info(),
//                     update_authority: ctx.accounts.collection_mint.to_account_info(),
//                     collection_metadata: ctx.accounts.collection_metadata.to_account_info(),
//                     collection_master_edition: ctx
//                         .accounts
//                         .collection_master_edition
//                         .to_account_info(),
//                     collection_mint: ctx.accounts.collection_mint.to_account_info(),
//                 },
//                 signer_seeds,
//             ),
//             None,
//         )?;

//         ctx.accounts.token_lottery.total_tickets += 1;

//         emit!(BoughtTicket {
//             price: ctx.accounts.token_lottery.ticket_price,
//             current_total_tickets: ctx.accounts.token_lottery.total_tickets
//         });

//         Ok(())
//     }

//     pub fn restart_lottery(
//         ctx: Context<RestartLottery>,
//         new_start_time: i64,
//         new_end_time: i64,
//         new_ticket_price: u64,
//     ) -> Result<()> {
//         let lottery = &mut ctx.accounts.token_lottery;
//         lottery.start_time = new_start_time;
//         lottery.end_time = new_end_time;
//         lottery.ticket_price = new_ticket_price;
//         lottery.total_tickets = 0;
//         lottery.winner_chosen = false;
//         lottery.winner = 0;
//         lottery.pot_amount = 0;
//         lottery.round_id += 1;
//         Ok(())
//     }

//     pub fn claim_winnings(ctx: Context<ClaimWinnings>) -> Result<()> {
//         // Check if winner has been chosen
//         msg!(
//             "Winner chosen: {}",
//             ctx.accounts.token_lottery.winner_chosen
//         );
//         require!(
//             ctx.accounts.token_lottery.winner_chosen,
//             ErrorCode::WinnerNotChosen
//         );

//         // Check if token is a part of the collection
//         require!(
//             ctx.accounts.metadata.collection.as_ref().unwrap().verified,
//             ErrorCode::NotVerifiedTicket
//         );
//         require!(
//             ctx.accounts.metadata.collection.as_ref().unwrap().key
//                 == ctx.accounts.collection_mint.key(),
//             ErrorCode::IncorrectTicket
//         );

//         let ticket_name = NAME.to_owned() + &ctx.accounts.token_lottery.winner.to_string();
//         let metadata_name = ctx.accounts.metadata.name.replace("\u{0}", "");

//         msg!("Ticket name: {}", ticket_name);
//         msg!("Metdata name: {}", metadata_name);

//         // Check if the winner has the winning ticket
//         require!(metadata_name == ticket_name, ErrorCode::IncorrectTicket);
//         require!(
//             ctx.accounts.destination.amount > 0,
//             ErrorCode::IncorrectTicket
//         );

//         let seeds = &[b"token_lottery".as_ref(), &[ctx.bumps.token_lottery]];
//         let signer = &[&seeds[..]];

//         let decimals = ctx.accounts.reward_mint.decimals;

//         let cpi_accounts = TransferChecked {
//             from: ctx.accounts.reward_vault.to_account_info(),
//             to: ctx.accounts.winner_token_account.to_account_info(),
//             mint: ctx.accounts.reward_mint.to_account_info(),
//             authority: ctx.accounts.token_lottery.to_account_info(),
//         };

//         let cpi_program = ctx.accounts.token_program.to_account_info();
//         let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);

//         transfer_checked(cpi_ctx, ctx.accounts.token_lottery.pot_amount, decimals)?;
//         ctx.accounts.token_lottery.pot_amount = 0;

//         emit!(WinningsClaimed {
//             ticket_name: ticket_name,
//             destination_account: ctx.accounts.destination.key()
//         });

//         Ok(())
//     }

//     pub fn commit_winner(ctx: Context<CommitWinner>, client_seed: u8) -> Result<()> {
//         let clock = Clock::get()?;
//         let token_lottery = &mut ctx.accounts.token_lottery;
//         if ctx.accounts.payer.key() != token_lottery.authority {
//             return Err(ErrorCode::NotAuthorized.into());
//         }

//         require!(
//             clock.unix_timestamp >= token_lottery.end_time,
//             ErrorCode::LotteryNotCompleted
//         );
//         require!(!token_lottery.winner_chosen, ErrorCode::WinnerChosen);

//         let clock = Clock::get()?;
//         msg!("Requesting randomness...");
//         let ix = create_request_randomness_ix(RequestRandomnessParams {
//             payer: ctx.accounts.payer.key(),
//             oracle_queue: ctx.accounts.oracle_queue.key(),
//             callback_program_id: ID,
//             callback_discriminator: instruction::CallbackChooseWinner::DISCRIMINATOR.to_vec(),
//             caller_seed: [client_seed; 32],
//             // Specify any account that is required by the callback
//             accounts_metas: Some(vec![SerializableAccountMeta {
//                 pubkey: ctx.accounts.token_lottery.key(),
//                 is_signer: false,
//                 is_writable: true,
//             }]),
//             ..Default::default()
//         });
//         ctx.accounts
//             .invoke_signed_vrf(&ctx.accounts.payer.to_account_info(), &ix)?;

//         emit!(WinnerCommited {
//             oracle_queue: ctx.accounts.oracle_queue.key()
//         });
//         Ok(())
//     }

//     pub fn callback_choose_winner(
//         ctx: Context<CallbackChooseWinnerCtx>,
//         randomness: [u8; 32],
//     ) -> Result<()> {
//         msg!("🎲 Callback invoked with randomness!");
//         let clock = Clock::get()?;

//         let token_lottery = &mut ctx.accounts.token_lottery;

//         // if clock.slot < token_lottery.end_time {
//         //     msg!("Current slot: {}", clock.slot);
//         //     msg!("End slot: {}", token_lottery.end_time);
//         //     return Err(ErrorCode::LotteryNotCompleted.into());
//         // }
//         require!(
//             token_lottery.winner_chosen == false,
//             ErrorCode::WinnerChosen
//         );

//         require!(
//             token_lottery.total_tickets > 0,
//             ErrorCode::LotteryNotCompleted
//         );

//         let random_number = ephemeral_vrf_sdk::rnd::random_u8_with_range(
//             &randomness,
//             0,
//             token_lottery.total_tickets as u8 - 1,
//         );
//         let winner_index = random_number as u64;
//         token_lottery.winner = winner_index;
//         token_lottery.winner_chosen = true;
//         emit!(SelectWinner {
//             winner: ctx.accounts.token_lottery.winner,
//             winner_chosen: ctx.accounts.token_lottery.winner_chosen
//         });

//         Ok(())
//     }

//     pub fn initialize_config(
//         ctx: Context<InitializeConfig>,
//         start_time: i64,
//         end_time: i64,
//         price: u64,
//     ) -> Result<()> {
//         ctx.accounts.token_lottery.bump = ctx.bumps.token_lottery;
//         ctx.accounts.token_lottery.start_time = start_time;
//         ctx.accounts.token_lottery.end_time = end_time;
//         ctx.accounts.token_lottery.ticket_price = price;
//         ctx.accounts.token_lottery.authority = ctx.accounts.signer.key();
//         ctx.accounts.token_lottery.pot_amount = 0;
//         ctx.accounts.token_lottery.winner_chosen = false;
//         ctx.accounts.token_lottery.round_id = 0;

//         emit!(InitializedConfig {
//             start_time: start_time,
//             end_time: end_time,
//             price: price,
//         });
//         Ok(())
//     }

//     pub fn initialize_lottery(ctx: Context<InitializeLottery>) -> Result<()> {
//         let signer_seeds: &[&[&[u8]]] =
//             &[&[b"collection_mint".as_ref(), &[ctx.bumps.collection_mint]]];
//         msg!("Creating Mint Account");
//         mint_to(
//             CpiContext::new_with_signer(
//                 ctx.accounts.token_program.to_account_info(),
//                 MintTo {
//                     mint: ctx.accounts.collection_mint.to_account_info(),
//                     to: ctx.accounts.collection_token_account.to_account_info(),
//                     authority: ctx.accounts.collection_mint.to_account_info(),
//                 },
//                 signer_seeds,
//             ),
//             1,
//         )?;
//         msg!("Creating Metadata Account v3");
//         create_metadata_accounts_v3(
//             CpiContext::new_with_signer(
//                 ctx.accounts.token_program.to_account_info(),
//                 CreateMetadataAccountsV3 {
//                     metadata: ctx.accounts.metadata.to_account_info(),
//                     mint: ctx.accounts.collection_mint.to_account_info(),
//                     mint_authority: ctx.accounts.collection_mint.to_account_info(),
//                     payer: ctx.accounts.payer.to_account_info(),
//                     update_authority: ctx.accounts.collection_mint.to_account_info(),
//                     system_program: ctx.accounts.system_program.to_account_info(),
//                     rent: ctx.accounts.rent.to_account_info(),
//                 },
//                 signer_seeds,
//             ),
//             DataV2 {
//                 name: NAME.to_string(),
//                 symbol: symbol.to_string(),
//                 uri: url.to_string(),
//                 seller_fee_basis_points: 0,
//                 creators: Some(vec![Creator {
//                     address: ctx.accounts.collection_mint.key(),
//                     verified: false,
//                     share: 100,
//                 }]),
//                 collection: None,
//                 uses: None,
//             },
//             true,
//             true,
//             Some(CollectionDetails::V1 { size: 0 }),
//         );

//         msg!("Creating Master Edition Account");

//         create_master_edition_v3(
//             CpiContext::new_with_signer(
//                 ctx.accounts.token_metadata_program.to_account_info(),
//                 CreateMasterEditionV3 {
//                     edition: ctx.accounts.master_edition.to_account_info(),
//                     mint: ctx.accounts.collection_mint.to_account_info(),
//                     update_authority: ctx.accounts.collection_mint.to_account_info(),
//                     mint_authority: ctx.accounts.collection_mint.to_account_info(),
//                     payer: ctx.accounts.payer.to_account_info(),
//                     metadata: ctx.accounts.metadata.to_account_info(),
//                     token_program: ctx.accounts.token_program.to_account_info(),
//                     system_program: ctx.accounts.system_program.to_account_info(),
//                     rent: ctx.accounts.rent.to_account_info(),
//                 },
//                 signer_seeds,
//             ),
//             Some(0),
//         );

//         msg!("Verifying Colleciton...");

//         sign_metadata(CpiContext::new_with_signer(
//             ctx.accounts.token_program.to_account_info(),
//             SignMetadata {
//                 creator: ctx.accounts.collection_mint.to_account_info(),
//                 metadata: ctx.accounts.metadata.to_account_info(),
//             },
//             signer_seeds,
//         ));

//         emit!(InitializedLottery {
//             collection_mint: ctx.accounts.collection_mint.key()
//         });
//         Ok(())
//     }
// }

// #[derive(Accounts)]
// pub struct InitializeLottery<'info> {
//     #[account(mut)]
//     pub payer: Signer<'info>,

//     #[account(init,
//     payer=payer,
//     mint::decimals=0,
//     mint::authority=collection_mint,
//     mint::freeze_authority=collection_mint,
//     seeds=[b"collection_mint", token_lottery.round_id.to_le_bytes().as_ref()],  // Add round_id
//     bump
// )]
//     pub collection_mint: InterfaceAccount<'info, Mint>,

//     #[account(
//         init,
//         payer=payer,
//        token::mint=collection_mint,
//         token::authority=collection_token_account,
//         seeds=[b"collection_associated_token".as_ref()],
//         bump
//     )]
//     pub collection_token_account: InterfaceAccount<'info, TokenAccount>,

//     #[account(
//             mut,
//             seeds=[b"metadata",
//             token_metadata_program.key().as_ref(), collection_mint.key().as_ref()], bump,
//             seeds::program=token_metadata_program.key()
//         )]
//     ///CHECK: These are checked by the token metadata program
//     pub metadata: UncheckedAccount<'info>,
//     #[account(
//         seeds=[b"token_lottery".as_ref()],
//         bump=token_lottery.bump
//     )]
//     pub token_lottery: Account<'info, TokenLottery>,

//     #[account(
//         mut,
//         seeds=[b"metadata", token_metadata_program.key().as_ref(), collection_mint.key().as_ref(), b"edition"],
//         bump,
//         seeds::program=token_metadata_program
//     )]
//     ///CHECK: These are checked by the token metadata program
//     pub master_edition: UncheckedAccount<'info>,
//     pub token_metadata_program: Program<'info, Metadata>,
//     pub associated_token_account: Program<'info, AssociatedToken>,
//     pub token_program: Interface<'info, TokenInterface>,
//     pub system_program: Program<'info, System>,
//     pub rent: Sysvar<'info, Rent>,
// }

// #[vrf]
// #[derive(Accounts)]
// pub struct CommitWinner<'info> {
//     #[account(mut)]
//     pub payer: Signer<'info>,

//     #[account(
//         mut,
//         seeds=[b"token_lottery".as_ref()],
//         bump=token_lottery.bump
//     )]
//     pub token_lottery: Account<'info, TokenLottery>,

//     /// CHECK: The oracle queue
//     #[account(mut, address = ephemeral_vrf_sdk::consts::DEFAULT_QUEUE)]
//     pub oracle_queue: AccountInfo<'info>,
//     pub system_program: Program<'info, System>,
// }

// #[derive(Accounts)]
// pub struct InitializeConfig<'info> {
//     #[account(mut)]
//     pub signer: Signer<'info>,

//     #[account(
//         init,
//         payer=signer,
//         space=8+ TokenLottery::INIT_SPACE,
//         seeds=[b"token_lottery".as_ref()],
//         bump
//     )]
//     pub token_lottery: Account<'info, TokenLottery>,

//     pub system_program: Program<'info, System>,
// }

// #[derive(Accounts)]
// pub struct ClaimWinnings<'info> {
//     #[account(mut)]
//     pub payer: Signer<'info>,

//     #[account(
//         mut,
//         seeds = [b"token_lottery".as_ref()],
//         bump
//     )]
//     pub token_lottery: Account<'info, TokenLottery>,

//     pub reward_mint: InterfaceAccount<'info, Mint>,

//     #[account(
//     mut,
//     associated_token::mint = reward_mint,
//     associated_token::authority = token_lottery,
//     associated_token::token_program = token_program,
// )]
//     pub reward_vault: InterfaceAccount<'info, TokenAccount>,

//     #[account(mut)]
//     pub winner_token_account: InterfaceAccount<'info, TokenAccount>,

//     #[account(
//         mut,
//         seeds = [b"collection_mint".as_ref()],
//         bump,
//     )]
//     pub collection_mint: InterfaceAccount<'info, Mint>,

//     #[account(
//         seeds = [token_lottery.winner.to_le_bytes().as_ref()],
//         bump,
//     )]
//     pub ticket_mint: InterfaceAccount<'info, Mint>,

//     #[account(
//         seeds = [b"metadata", token_metadata_program.key().as_ref(), ticket_mint.key().as_ref()],
//         bump,
//         seeds::program = token_metadata_program.key(),
//     )]
//     pub metadata: Account<'info, MetadataAccount>,

//     #[account(
//         associated_token::mint = ticket_mint,
//         associated_token::authority = payer,
//         associated_token::token_program = token_program,
//     )]
//     pub destination: InterfaceAccount<'info, TokenAccount>,

//     #[account(
//         mut,
//         seeds = [b"metadata", token_metadata_program.key().as_ref(), collection_mint.key().as_ref()],
//         bump,
//         seeds::program = token_metadata_program.key(),
//     )]
//     pub collection_metadata: Account<'info, MetadataAccount>,

//     pub token_program: Interface<'info, TokenInterface>,
//     pub system_program: Program<'info, System>,
//     pub token_metadata_program: Program<'info, Metadata>,
// }
// #[derive(Accounts)]
// pub struct RestartLottery<'info> {
//     #[account(
//         mut,
//         seeds = [b"token_lottery"],
//         bump = token_lottery.bump,
//     )]
//     pub token_lottery: Account<'info, TokenLottery>,

//     pub authority: Signer<'info>,
// }

// #[derive(Accounts)]
// pub struct CallbackChooseWinnerCtx<'info> {
//     /// This check ensure that the vrf_program_identity (which is a PDA) is a singer
//     /// enforcing the callback is executed by the VRF program trough CPI
//     #[account(address = ephemeral_vrf_sdk::consts::VRF_PROGRAM_IDENTITY)]
//     pub vrf_program_identity: Signer<'info>,

//     #[account(
//         mut,
//         // seeds = [b"token_lottery".as_ref()],
//         // bump = token_lottery.bump,
//     )]
//     pub token_lottery: Account<'info, TokenLottery>,
// }

// #[derive(Accounts)]
// pub struct BuyTicket<'info> {
//     #[account(mut)]
//     pub payer: Signer<'info>,

//     #[account(
//         mut,
//         seeds=[b"token_lottery".as_ref()],
//         bump=token_lottery.bump
//     )]
//     pub token_lottery: Box<Account<'info, TokenLottery>>,

//     #[account(
//         mut,
//         constraint = payer_token_account.mint == token_mint.key()
//         ,
//         constraint = payer_token_account.owner == payer.key(),
//     )]
//     pub payer_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

//     #[account(
//    init_if_needed,
//    payer = payer,
//    associated_token::mint = token_mint,
//    associated_token::authority = token_lottery,
//    associated_token::token_program = token_program,
// )]
//     pub raffle_vault_account: Box<InterfaceAccount<'info, TokenAccount>>,

//     pub token_mint: Box<InterfaceAccount<'info, Mint>>,

//     #[account(
//         init,
//         payer=payer,
//         seeds=[token_lottery.total_tickets.to_le_bytes().as_ref()],
//         bump,
//         mint::decimals=0,
//         mint::authority=collection_mint,
//         mint::freeze_authority=collection_mint,
//         mint::token_program=token_program
//     )]
//     pub ticket_mint: Box<InterfaceAccount<'info, Mint>>,

//     #[account(
//             mut,
//             seeds=[b"metadata",
//             token_metadata_program.key().as_ref(), ticket_mint.key().as_ref()], bump,
//             seeds::program=token_metadata_program.key()
//         )]
//     ///CHECK: These are checked by the token metadata program
//     pub ticket_metadata: UncheckedAccount<'info>,

//     #[account(
//         init,
//         payer=payer,
//         associated_token::mint=ticket_mint,
//         associated_token::authority=payer,
//         associated_token::token_program=token_program
//     )]
//     pub destination: Box<InterfaceAccount<'info, TokenAccount>>,

//     #[account(
//             mut,
//             seeds=[b"metadata",
//             token_metadata_program.key().as_ref(), collection_mint.key().as_ref()], bump,
//             seeds::program=token_metadata_program.key()
//         )]
//     ///CHECK: These are checked by the token metadata program
//     pub collection_metadata: UncheckedAccount<'info>,

//     #[account(
//         mut,
//         seeds=[b"metadata", token_metadata_program.key().as_ref(), ticket_mint.key().as_ref(), b"edition"],
//         bump,
//         seeds::program=token_metadata_program
//     )]
//     ///CHECK: These are checked by the token metadata program
//     pub ticket_master_edition: UncheckedAccount<'info>,

//     #[account(
//         mut,
//         seeds=[b"metadata", token_metadata_program.key().as_ref(), collection_mint.key().as_ref(), b"edition"],
//         bump,
//         seeds::program=token_metadata_program
//     )]
//     ///CHECK: These are checked by the token metadata program
//     pub collection_master_edition: UncheckedAccount<'info>,

//     pub token_metadata_program: Program<'info, Metadata>,

//     #[account(
//         mut,
//         seeds=[b"collection_mint".as_ref(), token_lottery.round_id.to_le_bytes().as_ref()],
//         bump
//     )]
//     pub collection_mint: Box<InterfaceAccount<'info, Mint>>,

//     #[account(
//         mut,
//         seeds = [b"metadata", token_metadata_program.key().as_ref(),
//         ticket_mint.key().as_ref()],
//         bump,
//         seeds::program = token_metadata_program.key(),
//     )]
//     /// CHECK: This account will be initialized by the metaplex program
//     pub metadata: UncheckedAccount<'info>,

//     #[account(
//         mut,
//         seeds = [b"metadata", token_metadata_program.key().as_ref(),
//             ticket_mint.key().as_ref(), b"edition"],
//         bump,
//         seeds::program = token_metadata_program.key(),
//     )]
//     /// CHECK: This account will be initialized by the metaplex program
//     pub master_edition: UncheckedAccount<'info>,

//     pub associated_token_program: Program<'info, AssociatedToken>,
//     pub token_program: Interface<'info, TokenInterface>,
//     pub system_program: Program<'info, System>,
//     pub rent: Sysvar<'info, Rent>,
// }

// #[error_code]
// pub enum ErrorCode {
//     #[msg("Lottery is not open yet.")]
//     LotteryNotOpen,
//     #[msg("You're Not Authorized!.")]
//     NotAuthorized,
//     #[msg("Randomnes Already Revealed")]
//     RandomnessAlreadyRevealed,
//     #[msg("Lottery Not Completed")]
//     LotteryNotCompleted,
//     #[msg("Incorrect Randomness Account")]
//     IncorrectRandomnessAccount,
//     #[msg("Randomness Not Resolved")]
//     RandomnessNotResolved,
//     #[msg("Winner Already Chosen")]
//     WinnerChosen,
//     #[msg("Ticket Is Incorrect")]
//     IncorrectTicket,
//     #[msg("Winner Not Chosen")]
//     WinnerNotChosen,
//     #[msg("Ticket Not Verified")]
//     NotVerifiedTicket,
// }

// #[event]
// pub struct InitializedConfig {
//     pub start_time: i64,
//     pub end_time: i64,
//     pub price: u64,
// }
// #[event]
// pub struct InitializedLottery {
//     pub collection_mint: Pubkey,
// }

// #[event]
// pub struct BoughtTicket {
//     pub price: u64,
//     pub current_total_tickets: u64,
// }
// #[event]
// pub struct SelectWinner {
//     pub winner: u64,
//     pub winner_chosen: bool,
// }
// #[event]
// pub struct WinningsClaimed {
//     pub ticket_name: String,
//     pub destination_account: Pubkey,
// }

// #[event]
// pub struct WinnerCommited {
//     pub oracle_queue: Pubkey,
// }

// #[account]
// #[derive(InitSpace)]
// pub struct TokenLottery {
//     pub round_id: u64,
//     pub winner: u64,
//     pub winner_chosen: bool,
//     pub start_time: i64,
//     pub end_time: i64,
//     pub pot_amount: u64,
//     pub total_tickets: u64,
//     pub ticket_price: u64,
//     pub authority: Pubkey,
//     pub bump: u8,
// }
