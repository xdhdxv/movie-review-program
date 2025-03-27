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
    native_token::sol_to_lamports,
    program_pack::Pack,
};

use spl_token::{
    ID as TOKEN_PROGRAM_ID,
    instruction::initialize_mint2,
};

use spl_associated_token_account::get_associated_token_address;

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
        },
        MovieInstruction::InitializeMint => {
            initialize_token_mint(program_id, accounts)
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
    let token_mint = next_account_info(account_info_iter)?;
    let mint_auth = next_account_info(account_info_iter)?;
    let user_ata = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

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

    if MovieAccountState::get_account_size(title.clone(), description.clone()) > MovieAccountState::LEN {
        msg!("Data length is larger than 1000 bytes");
        return Err(ReviewError::InvalidDataLength.into())
    }

    let rent = Rent::get()?;
    let rent_lamports = rent.minimum_balance(MovieAccountState::LEN);

    invoke_signed(
        &system_instruction::create_account(
            initializer.key, 
            pda_account.key, 
            rent_lamports, 
            MovieAccountState::LEN.try_into().unwrap(), 
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
    let counter_rent_lamports = rent.minimum_balance(MovieCommentCounter::LEN);

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
            MovieCommentCounter::LEN.try_into().unwrap(), 
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

    msg!("Deriving mint authority");
    let (mint_pda, _mint_bump) = Pubkey::find_program_address(&[b"token_mint"], program_id);
    let (mint_auth_pda, mint_auth_bump) = Pubkey::find_program_address(&[b"token_auth"], program_id);

    if mint_pda != *token_mint.key {
        msg!("Incorrect token mint");
        return Err(ReviewError::IncorrectAccount.into());
    }

    if mint_auth_pda != *mint_auth.key {
        msg!("Mint authority passed in and mint authority derived do not match");
        return Err(ReviewError::InvalidPDA.into());
    }

    if get_associated_token_address(initializer.key, token_mint.key) != *user_ata.key {
        msg!("Incorrect ATA for initializer");
        return Err(ReviewError::IncorrectAccount.into());
    }

    if TOKEN_PROGRAM_ID != *token_program.key {
        msg!("Incorrect token program");
        return Err(ReviewError::IncorrectAccount.into());
    }

    msg!("Minting 10 tokens to User ATA");
    invoke_signed(
        &spl_token::instruction::mint_to(
            token_program.key, 
            token_mint.key, 
            user_ata.key, 
            mint_auth.key, 
            &[], 
            sol_to_lamports(10.0)
        )?, 
        &[token_mint.clone(), user_ata.clone(), mint_auth.clone()], 
        &[&[b"token_auth", &[mint_auth_bump]]],
    )?;

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

    if MovieAccountState::get_account_size(title.clone(), description.clone()) > MovieAccountState::LEN {
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
    let token_mint = next_account_info(account_info_iter)?;
    let mint_auth = next_account_info(account_info_iter)?;
    let user_ata = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

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

    msg!("Deriving mint authority");
    let (mint_pda, _mint_bump) = Pubkey::find_program_address(&[b"token_mint"], program_id);
    let (mint_auth_pda, mint_auth_bump) = Pubkey::find_program_address(&[b"token_auth"], program_id);

    if mint_pda != *token_mint.key {
        msg!("Incorrect token mint");
        return Err(ReviewError::IncorrectAccount.into());
    }

    if mint_auth_pda != *mint_auth.key {
        msg!("Mint authority passed in and mint authority derived do not match");
        return Err(ReviewError::InvalidPDA.into());
    }

    if get_associated_token_address(commenter.key, token_mint.key) != *user_ata.key {
        msg!("Incorrect ATA for commenter");
        return Err(ReviewError::IncorrectAccount.into());
    }

    if TOKEN_PROGRAM_ID != *token_program.key {
        msg!("Incorrect token program");
        return Err(ReviewError::IncorrectAccount.into());
    }

    msg!("Minting 5 tokens to User ATA");
    invoke_signed(
        &spl_token::instruction::mint_to(
            token_program.key, 
            token_mint.key, 
            user_ata.key, 
            mint_auth.key, 
            &[], 
            sol_to_lamports(5.0)
        )?, 
        &[token_mint.clone(), user_ata.clone(), mint_auth.clone()], 
        &[&[b"token_auth", &[mint_auth_bump]]],
    )?;

    Ok(())
}   

pub fn initialize_token_mint(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let initializer = next_account_info(account_info_iter)?;
    let token_mint = next_account_info(account_info_iter)?;
    let mint_auth = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    let (mint_pda, mint_bump) = Pubkey::find_program_address(&[b"token_mint"], program_id);
    let (mint_auth_pda, _mint_auth_bump) = Pubkey::find_program_address(&[b"token_auth"], program_id);

    msg!("Token mint: {:?}", mint_pda);
    msg!("Mint authority: {:?}", mint_auth_pda);

    if mint_pda != *token_mint.key {
        msg!("Incorrect token mint account");
        return Err(ReviewError::IncorrectAccount.into());
    }

    if TOKEN_PROGRAM_ID != *token_program.key {
        msg!("Incorrect token program");
        return Err(ReviewError::IncorrectAccount.into());
    }

    if mint_auth_pda != *mint_auth.key {
        msg!("Incorrect mint auth account");
        return Err(ReviewError::IncorrectAccount.into());
    }

    let rent = Rent::get()?;
    let rent_lamports = rent.minimum_balance(spl_token::state::Mint::LEN);

    invoke_signed(
        &system_instruction::create_account(
            initializer.key, 
            token_mint.key, 
            rent_lamports, 
            spl_token::state::Mint::LEN.try_into().unwrap(), 
            token_program.key,
        ), 
        &[
            initializer.clone(),
            token_mint.clone(),
            system_program.clone(),
        ], 
        &[&[b"token_mint", &[mint_bump]]],
    )?;

    msg!("Created token mint account");

    invoke_signed(
        &initialize_mint2(
            token_program.key, 
            token_mint.key, 
            mint_auth.key, 
            None, 
            9,
        )?, 
        &[
            token_mint.clone(),
            mint_auth.clone(),
        ],
        &[&[b"token_mint", &[mint_bump]]], 
    )?;

    msg!("Initialized token mint");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use borsh::BorshDeserialize;

    use solana_program_test::*;

    use solana_sdk::{
        signature::Signer,
        instruction::{Instruction, AccountMeta},
        system_program,
        transaction::Transaction,
    };

    #[tokio::test]
    async fn test_initialize_mint_instruction() {
        let program_id = Pubkey::new_unique();

        let mut program_test = ProgramTest::default();
        program_test.add_program(
            "movie_review_program",
            program_id,
            processor!(process_instruction)
        );

        let (mut banks_client, payer, recent_blockhash) = program_test.start().await;

        let (_mint, _mint_auth, init_mint_ix) = create_init_mint_ix(payer.pubkey(), &program_id);

        let mut transaction = Transaction::new_with_payer(
            &[init_mint_ix], 
            Some(&payer.pubkey())
        );
        transaction.sign(&[&payer], recent_blockhash);

        let transaction_result = banks_client.process_transaction(transaction).await;

        assert!(transaction_result.is_ok());
    }

    #[tokio::test]
    async fn test_add_movie_review_instruction() {
        let program_id = Pubkey::new_unique();

        let mut program_test = ProgramTest::default();
        program_test.add_program(
            "movie_review_program", 
            program_id, 
            processor!(process_instruction)
        );
        
        let (mut banks_client, payer, recent_blockhash) = program_test.start().await;

        let (mint, mint_auth, init_mint_ix) = create_init_mint_ix(payer.pubkey(), &program_id);

        let title = String::from("Captain America");
        let rating: u8 = 3;
        let description =  String::from("Liked the movie");

        let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
            &payer.pubkey(), 
            &payer.pubkey(), 
            &mint, 
            &spl_token::ID
        );

        let user_ata = spl_associated_token_account::get_associated_token_address(
            &payer.pubkey(), &mint
        );

        let add_movie_review_ix = create_add_movie_review_ix(
            payer.pubkey(), 
            program_id, 
            title, 
            rating, 
            description, 
            mint, 
            mint_auth, 
            user_ata, 
            system_program::ID, 
            spl_token::ID
        );

        let mut transaction = Transaction::new_with_payer(
            &[init_mint_ix, create_ata_ix, add_movie_review_ix], 
            Some(&payer.pubkey())
        );

        transaction.sign(&[&payer], recent_blockhash);

        let transaction_result = banks_client.process_transaction(transaction).await;

        assert!(transaction_result.is_ok());
    }

    #[tokio::test]
    async fn test_update_movie_review_instruction() {
        let program_id = Pubkey::new_unique();

        let mut program_test = ProgramTest::default();
        program_test.add_program(
            "movie_review_program", 
            program_id, 
            processor!(process_instruction)
        );

        let (mut banks_client, payer, recent_blockhash) = program_test.start().await;

        let (mint, mint_auth, init_mint_ix) = create_init_mint_ix(
            payer.pubkey(),
            &program_id
        );

        let title = String::from("Captain America");
        let rating: u8 = 3;
        let description = String::from("Liked the movie");

        let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
            &payer.pubkey(), 
            &payer.pubkey(), 
            &mint, 
            &spl_token::ID
        );

        let user_ata = spl_associated_token_account::get_associated_token_address(
            &payer.pubkey(), &mint
        );

        let add_movie_review_ix = create_add_movie_review_ix(
            payer.pubkey(), 
            program_id, 
            title.clone(), 
            rating, 
            description, 
            mint, 
            mint_auth, 
            user_ata, 
            system_program::ID, 
            spl_token::ID
        );

        let mut transaction = Transaction::new_with_payer(
            &[init_mint_ix, create_ata_ix, add_movie_review_ix], 
            Some(&payer.pubkey())
        );

        transaction.sign(&[&payer], recent_blockhash);

        banks_client.process_transaction(transaction).await.unwrap();

        let new_rating: u8 = 2;
        let new_description =  String::from("Didn't like the movie");
        
        let update_movie_review_ix = create_update_movie_instruction(
            payer.pubkey(), 
            program_id, 
            title.clone(), 
            new_rating, 
            new_description,
        );

        let mut transaction = Transaction::new_with_payer(
            &[update_movie_review_ix], 
            Some(&payer.pubkey())
        );

        transaction.sign(&[&payer], recent_blockhash);

        let transaction_result = banks_client.process_transaction(transaction).await;

        assert!(transaction_result.is_ok());
    }

    #[tokio::test]
    async fn test_add_comment_instruction() {
        let program_id = Pubkey::new_unique();

        let mut program_test = ProgramTest::default();
        program_test.add_program(
            "movie_review_program", 
            program_id, 
            processor!(process_instruction)
        );

        let (mut banks_client, payer, recent_blockhash) = program_test.start().await;

        let (mint, mint_auth, init_mint_ix) = create_init_mint_ix(
            payer.pubkey(), &program_id
        );

        let title = String::from("Captain America");
        let rating: u8 = 3;
        let description = String::from("Liked the movie");
        
        let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
            &payer.pubkey(), 
            &payer.pubkey(), 
            &mint, 
            &spl_token::ID,
        );

        let user_ata = spl_associated_token_account::get_associated_token_address(
            &payer.pubkey(), 
            &mint
        );

        let add_movie_review_ix = create_add_movie_review_ix(
            payer.pubkey(), 
            program_id, 
            title.clone(), 
            rating, 
            description, 
            mint, 
            mint_auth, 
            user_ata, 
            system_program::ID, 
            spl_token::ID
        );

        let mut transaction = Transaction::new_with_payer(
            &[init_mint_ix, create_ata_ix, add_movie_review_ix], 
            Some(&payer.pubkey()),
        );

        transaction.sign(&[&payer], recent_blockhash);

        banks_client.process_transaction(transaction).await.unwrap();

        let comment = String::from("Totally agree!");

        let (review_pda, _review_bump) = Pubkey::find_program_address(
            &[payer.pubkey().as_ref(), title.as_bytes()], 
            &program_id
        );

        let (counter_pda, _counter_bump) = Pubkey::find_program_address(
            &[review_pda.as_ref(), b"comment"], 
            &program_id
        );

        let counter_account = banks_client.get_account(counter_pda).await.unwrap().unwrap();

        let counter_data: MovieCommentCounter = try_from_slice_unchecked(&counter_account.data).unwrap();
        
        let add_comment_ix = create_add_comment_instruction(
            payer.pubkey(), 
            program_id,
            title.clone(),
            comment, 
            counter_data.counter, 
            mint, 
            mint_auth, 
            user_ata, 
            system_program::ID, 
            spl_token::ID,
        );

        let mut transaction = Transaction::new_with_payer(
            &[add_comment_ix], 
            Some(&payer.pubkey())
        );

        transaction.sign(&[&payer], recent_blockhash);

        let transaction_result = banks_client.process_transaction(transaction).await;

        assert!(transaction_result.is_ok());
    }

    fn create_init_mint_ix(payer: Pubkey, program_id: &Pubkey) -> (Pubkey, Pubkey, Instruction) {
        let (mint, _mint_bump) = Pubkey::find_program_address(
            &[b"token_mint"], program_id
        );
        let (mint_auth, _mint_auth_bump) = Pubkey::find_program_address(
            &[b"token_auth"], 
            program_id
        );

        let init_mint_ix = Instruction::new_with_borsh(
            *program_id, 
            &3, 
            vec![
                AccountMeta::new_readonly(payer, true),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_auth, false),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(spl_token::ID, false),
            ],
        );

        (mint, mint_auth, init_mint_ix)
    }

    fn create_add_movie_review_ix(
        payer: Pubkey,
        program_id: Pubkey,
        title: String,
        rating: u8,
        description: String,
        mint: Pubkey,
        mint_auth: Pubkey,
        user_ata: Pubkey,
        system_program: Pubkey,
        token_program: Pubkey
    ) -> Instruction {
        let (review_pda, _review_bump) = Pubkey::find_program_address(
            &[payer.as_ref(), title.as_bytes()], 
            &program_id
        );

        let (counter_pda, _counter_bump) = Pubkey::find_program_address(
            &[review_pda.as_ref(), b"comment"], 
            &program_id
        );

        let movie_review_payload = MovieReviewPayload {
            discriminator: 0,
            title,
            rating,
            description
        };

        Instruction::new_with_borsh(
            program_id, 
            &movie_review_payload, 
            vec![
                AccountMeta::new_readonly(payer, true),
                AccountMeta::new(review_pda, false),
                AccountMeta::new(counter_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_auth, false),
                AccountMeta::new(user_ata, false),
                AccountMeta::new_readonly(system_program, false),
                AccountMeta::new_readonly(token_program, false),
            ]
        )
    }

    fn create_update_movie_instruction(
        payer: Pubkey,
        program_id: Pubkey,
        title: String,
        rating: u8,
        description: String,
    ) -> Instruction {
        let (review_pda, _review_bump) = Pubkey::find_program_address(
            &[payer.as_ref(), title.as_bytes()], &program_id
        );

        let movie_review_payload = MovieReviewPayload {
            discriminator: 1,
            title,
            rating,
            description,
        };

        Instruction::new_with_borsh(
            program_id, 
            &movie_review_payload, 
            vec![
                AccountMeta::new_readonly(payer, true),
                AccountMeta::new(review_pda, false)
            ]
        )
    }

    fn create_add_comment_instruction(
        payer: Pubkey,
        program_id: Pubkey,
        title: String,
        comment: String,
        comment_count: u64,
        mint: Pubkey,
        mint_auth: Pubkey,
        user_ata: Pubkey,
        system_program: Pubkey,
        token_program: Pubkey
    ) -> Instruction {
        let (review_pda, _review_bump) = Pubkey::find_program_address(
            &[payer.as_ref(), title.as_bytes()], 
            &program_id
        );

        let (counter_pda, _counter_bump) = Pubkey::find_program_address(
            &[review_pda.as_ref(), b"comment"], 
            &program_id
        );

        let (comment_pda, _comment_bump) = Pubkey::find_program_address(
            &[review_pda.as_ref(), &comment_count.to_be_bytes()], 
            &program_id
        );

        let comment_payload = CommentPayload {
            discriminator: 2,
            comment,
        };

        Instruction::new_with_borsh(
            program_id, 
            &comment_payload, 
            vec![
                AccountMeta::new_readonly(payer, true),
                AccountMeta::new_readonly(review_pda, false),
                AccountMeta::new(counter_pda, false),
                AccountMeta::new(comment_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_auth, false),
                AccountMeta::new(user_ata, false),
                AccountMeta::new_readonly(system_program, false),
                AccountMeta::new_readonly(token_program, false),
            ]
        )
    }

    #[derive(BorshSerialize)]
    struct MovieReviewPayload {
        discriminator: u8,
        title: String,
        rating: u8,
        description: String,
    }

    #[derive(BorshSerialize)]
    struct CommentPayload {
        discriminator: u8,
        comment: String,
    }

    #[derive(BorshDeserialize, Debug)]
    struct MovieCommentCounter {
        discriminator: String,
        is_initialized: bool,
        counter: u64,
    }
}