//! Program error codes.

use solana_program::{decode_error::DecodeError, program_error::ProgramError};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BondhiveError {
    #[error("instruction unpack failed")]
    InvalidInstruction,
    #[error("not rent-exempt / bad account owner")]
    InvalidAccount,
    #[error("pool already initialized")]
    AlreadyInitialized,
    #[error("pool not initialized")]
    NotInitialized,
    #[error("signer mismatch")]
    SignerMismatch,
    #[error("bond not found")]
    BondNotFound,
    #[error("bond in wrong state for this action")]
    WrongBondState,
    #[error("insufficient staked balance")]
    InsufficientStaked,
    #[error("insufficient treasury balance")]
    InsufficientTreasury,
    #[error("bond value exceeds score tier cap")]
    BondTooLarge,
    #[error("invalid verdict or outcome value")]
    InvalidVerdict,
    #[error("arithmetic overflow")]
    Overflow,
    #[error("unauthorized authority")]
    Unauthorized,
    #[error("witness has already voted on this bond")]
    AlreadyVoted,
}

impl From<BondhiveError> for ProgramError {
    fn from(e: BondhiveError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl<T> DecodeError<T> for BondhiveError {
    fn type_of() -> &'static str {
        "BondhiveError"
    }
}
