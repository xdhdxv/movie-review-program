use borsh::{BorshSerialize, BorshDeserialize};

use solana_program::{
    pubkey::Pubkey,
    program_pack::{IsInitialized, Sealed},
};

#[derive(BorshSerialize, BorshDeserialize)]
pub struct MovieAccountState {
    pub discriminator: String,
    pub is_initialized: bool,
    pub reviewer: Pubkey,
    pub rating: u8,
    pub title: String,
    pub description: String,
}

impl MovieAccountState {
    pub const DISCRIMINATOR: &'static str = "review";

    pub fn get_account_size(title: String, description: String) -> usize {
        (4 + MovieAccountState::DISCRIMINATOR.len())
        + 1
        + 32
        + 1
        + (4 + title.len())
        + (4 + description.len())
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct MovieCommentCounter {
    pub discriminator: String,
    pub is_initialized: bool,
    pub counter: u64,
}

impl MovieCommentCounter {
    pub const DISCRIMINATOR: &'static str = "counter";
    
    pub const SIZE: usize =  (4 + MovieCommentCounter::DISCRIMINATOR.len())
        + 1
        + 8;
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct MovieComment {
    pub discriminator: String,
    pub is_initialized: bool,
    pub review: Pubkey,
    pub commenter: Pubkey,
    pub comment: String,
    pub count: u64,
}

impl MovieComment {
    pub const DISCRIMINATOR: &'static str = "comment";

    pub fn get_account_size(comment: String) -> usize {
        (4 + MovieComment::DISCRIMINATOR.len())
        + 1
        + 32
        + 32
        + (4 + comment.len())
        + 8
    }
}

impl Sealed for MovieAccountState {}

impl IsInitialized for MovieAccountState {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl IsInitialized for MovieCommentCounter {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl IsInitialized for MovieComment {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}
