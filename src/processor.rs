use solana_program::{
    pubkey::Pubkey,
    account_info::{AccountInfo, next_account_info},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    rent::Rent,
    sysvar::Sysvar,
    program::invoke_signed,
    system_instruction,
    borsh1::try_from_slice_unchecked,
    program_pack::IsInitialized,
};

use borsh::BorshSerialize;

use crate::instruction::MovieInstruction;
use crate::state::{MovieAccountState, MovieCommentCounter, MovieComment};
use crate::error::ReviewError;

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8]
) -> ProgramResult {
    let instruction = MovieInstruction::unpack(instruction_data)?;

    match instruction {
        MovieInstruction::AddMovieReview { title, rating, description } => {
            add_movie_review(program_id, accounts, title, rating, description)
        },
        MovieInstruction::UpdateMovieReview { title, rating, description } => {
            update_movie_review(program_id, accounts, title, rating, description)
        },
        MovieInstruction::AddComment { comment } => {
            add_comment(program_id, accounts, comment)
        }
    }
}

pub fn add_movie_review(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    title: String,
    rating: u8,
    description: String,
) -> ProgramResult {
    msg!("Adding movie review...");
    msg!("Title: {}", title);
    msg!("Rating: {}", rating);
    msg!("Description: {}", description);

    let account_info_iter = &mut accounts.iter();

    let initializer = next_account_info(account_info_iter)?;
    let pda_account = next_account_info(account_info_iter)?;
    let pda_counter = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    if !initializer.is_signer {
        msg!("Missing required signature");
        return Err(ProgramError::MissingRequiredSignature)
    }

    let (pda, bump_seed) = Pubkey::find_program_address(
        &[initializer.key.as_ref(), title.as_bytes().as_ref()], 
        program_id,
    );

    if pda != *pda_account.key {
        msg!("Invalid seeds for PDA");
        return Err(ReviewError::InvalidPDA.into())
    }

    if rating > 5 || rating < 1 {
        msg!("Rating cannot be higher than 5");
        return Err(ReviewError::InvalidRating.into())
    }

    let account_len: usize = 1000;
    if MovieAccountState::get_account_size(title.clone(), description.clone()) > account_len {
        msg!("Data length is larger than 1000 bytes");
        return Err(ReviewError::InvalidDataLength.into())
    }

    let rent = Rent::get()?;
    let rent_lamports = rent.minimum_balance(account_len);

    invoke_signed(
        &system_instruction::create_account(
            initializer.key, 
            pda_account.key, 
            rent_lamports, 
            account_len.try_into().unwrap(), 
            program_id
        ), 
        &[
            initializer.clone(),
            pda_account.clone(),
            system_program.clone(),
        ], 
        &[&[
            initializer.key.as_ref(),
            title.as_bytes().as_ref(),
            &[bump_seed]
        ]],
    )?;

    msg!("PDA created: {}", pda);

    msg!("Unpacking account");
    let mut account_data: MovieAccountState = try_from_slice_unchecked(&pda_account.data.borrow())?;
    msg!("Borrowed account data");

    msg!("Checking if movie account is already initialized");
    if account_data.is_initialized {
        msg!("Account already initialized");
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    account_data.discriminator = MovieAccountState::DISCRIMINATOR.to_string();
    account_data.reviewer = *initializer.key;
    account_data.title = title;
    account_data.rating = rating;
    account_data.description = description;
    account_data.is_initialized = true;

    msg!("Serializing account");
    account_data.serialize(&mut &mut pda_account.data.borrow_mut()[..])?;
    msg!("State account serialized");

    msg!("Create comment counter");
    let rent = Rent::get()?;
    let counter_rent_lamports = rent.minimum_balance(MovieCommentCounter::SIZE);

    let (counter, counter_bump) = Pubkey::find_program_address(
        &[pda.as_ref(), b"comment"], 
        program_id
    );

    if counter != *pda_counter.key {
        msg!("Invalid seeds for PDA");
        return Err(ReviewError::InvalidPDA.into());
    }

    invoke_signed(
        &system_instruction::create_account(
            initializer.key, 
            pda_counter.key, 
            counter_rent_lamports, 
            MovieCommentCounter::SIZE.try_into().unwrap(), 
            program_id
        ), 
        &[
            initializer.clone(),
            pda_counter.clone(),
            system_program.clone()
        ], 
        &[&[pda.as_ref(), b"comment", &[counter_bump]]],
    )?;
    msg!("Comment counter created");

    let mut counter_data: MovieCommentCounter =  
        try_from_slice_unchecked(&pda_counter.data.borrow())?;

    msg!("Checking if counter account is already initialized");
    if counter_data.is_initialized() {
        msg!("Account already initialized");
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    counter_data.discriminator = MovieCommentCounter::DISCRIMINATOR.to_string();
    counter_data.counter = 0;
    counter_data.is_initialized = true;
    
    msg!("Comment count: {}", counter_data.counter);

    counter_data.serialize(&mut &mut pda_counter.data.borrow_mut()[..])?;

    Ok(())
}

pub fn update_movie_review(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    title: String,
    rating: u8,
    description: String
) -> ProgramResult {
    msg!("Updating movie review...");

    let account_info_iter = &mut accounts.iter();

    let initializer = next_account_info(account_info_iter)?;
    let pda_account = next_account_info(account_info_iter)?;

    if pda_account.owner != program_id {
        return Err(ProgramError::InvalidAccountOwner)
    }

    if !initializer.is_signer {
        msg!("Missing required signature");
        return Err(ProgramError::MissingRequiredSignature);
    }

    msg!("Unpacking state account");
    let mut account_data: MovieAccountState = try_from_slice_unchecked(&pda_account.data.borrow())?;
    msg!("Review title: {}", account_data.title);

    let (pda, _bump_seed) = Pubkey::find_program_address(
        &[initializer.key.as_ref(), account_data.title.as_bytes().as_ref()], 
        program_id
    );
    if pda != *pda_account.key {
        msg!("Invalid seeds for PDA");
        return Err(ReviewError::InvalidPDA.into());
    }

    msg!("Checking if movie account is initialized");
    if !account_data.is_initialized() {
        msg!("Account is not initialized");
        return Err(ReviewError::UninitializedAccount.into());
    }

    if rating > 5 || rating < 1 {
        msg!("Rating cannot be higher than 5");
        return Err(ReviewError::InvalidRating.into());
    }

    let account_len: usize = 1000;
    if MovieAccountState::get_account_size(title.clone(), description.clone()) > account_len {
        msg!("Data length is larger than 1000 bytes");
        return Err(ReviewError::InvalidDataLength.into());
    }

    msg!("Review before update:");
    msg!("Title: {}", account_data.title);
    msg!("Rating: {}", account_data.rating);
    msg!("Description: {}", account_data.description);

    account_data.rating = rating;
    account_data.description = description;

    msg!("Review after update:");
    msg!("Title: {}", account_data.title);
    msg!("Rating: {}", account_data.rating);
    msg!("Description: {}", account_data.description);

    msg!("Serializing account");
    account_data.serialize(&mut &mut pda_account.data.borrow_mut()[..])?;
    msg!("State account serialized");

    Ok(())
}

pub fn add_comment(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    comment: String
) -> ProgramResult {
    msg!("Adding Comment...");
    msg!("Comment: {}", comment);

    let account_info_iter = &mut accounts.iter();

    let commenter = next_account_info(account_info_iter)?;
    let pda_review = next_account_info(account_info_iter)?;
    let pda_counter = next_account_info(account_info_iter)?;
    let pda_comment = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    let mut counter_data: MovieCommentCounter = 
        try_from_slice_unchecked(&pda_counter.data.borrow())?;

    let account_len: usize = MovieComment::get_account_size(comment.clone());

    let rent = Rent::get()?;
    let rent_lamports = rent.minimum_balance(account_len);

    let (pda, bump_seed) = Pubkey::find_program_address(
        &[
            pda_review.key.as_ref(),
            counter_data.counter.to_be_bytes().as_ref(),
        ], 
        program_id,
    );

    if pda != *pda_comment.key {
        msg!("Invalid seeds for PDA");
        return Err(ReviewError::InvalidPDA.into());
    }

    invoke_signed(
        &system_instruction::create_account(
            commenter.key, 
            pda_comment.key, 
            rent_lamports, 
            account_len.try_into().unwrap(), 
            program_id
        ), 
        &[
            commenter.clone(),
            pda_comment.clone(),
            system_program.clone(),
        ], 
        &[&[
            pda_review.key.as_ref(),
            counter_data.counter.to_be_bytes().as_ref(),
            &[bump_seed],
        ]]
    )?;

    msg!("Created Comment Account");

    let mut comment_data: MovieComment = 
        try_from_slice_unchecked(&pda_comment.data.borrow())?;

    msg!("Checking if comment is already initialized");
    if comment_data.is_initialized() {
        msg!("Account already initialized");
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    comment_data.discriminator = MovieComment::DISCRIMINATOR.to_string();
    comment_data.review = *pda_review.key;
    comment_data.commenter = *commenter.key;
    comment_data.comment = comment;
    comment_data.is_initialized = true;
    
    comment_data.serialize(&mut &mut pda_comment.data.borrow_mut()[..])?;

    msg!("Comment Count: {}", counter_data.counter);
    counter_data.counter += 1;
    counter_data.serialize(&mut &mut pda_counter.data.borrow_mut()[..])?;

    Ok(())
}