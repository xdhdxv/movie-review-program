use solana_program::program_error::ProgramError;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReviewError {
    // Error 0
    #[error("Account not initialized yet")]
    UninitializedAccount,
    // Error 1
    #[error("PDA derived does not equal PDA passed in")]
    InvalidPDA,
    // Error 2
    #[error("Input data exceeds max length")]
    InvalidDataLength,
    // Error 3
    #[error("Rating greater than 5 or less than 1")]
    InvalidRating,
    // Error 4
    #[error("Account do not match")]
    IncorrectAccount,
}

impl From<ReviewError> for ProgramError {
    fn from(value: ReviewError) -> Self {
        ProgramError::Custom(value as u32)
    }
}