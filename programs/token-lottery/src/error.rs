use anchor_lang::prelude::*;

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
