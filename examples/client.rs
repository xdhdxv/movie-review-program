use borsh::BorshSerialize;

use solana_cli_config::{CONFIG_FILE, Config};

use solana_client::rpc_client::RpcClient;

use solana_sdk::{
    signature::{keypair, Signer}, 
    instruction::Instruction,
    transaction::Transaction,
};

fn main() {
    let config_file = CONFIG_FILE.as_ref().unwrap();
    let config = Config::load(config_file).unwrap();

    let client = RpcClient::new(config.json_rpc_url);
    let program_id = keypair::read_keypair_file("target/deploy/movie_review_program-keypair.json").unwrap().pubkey();
    let payer = keypair::read_keypair_file(config.keypair_path).unwrap();

    let movie_review_payload = MovieReviewPayload {
        discriminator: 0,
        title: String::from("title"),
        rating: 10,
        description: String::from("description")
    };

    let instruction = Instruction::new_with_borsh(
        program_id, 
        &movie_review_payload, 
        vec![]
    );

    let transaction = Transaction::new_signed_with_payer(
        &[instruction], 
        Some(&payer.pubkey()), 
        &[&payer], 
        client.get_latest_blockhash().unwrap()
    );

    let tx_signature = client.send_and_confirm_transaction_with_spinner(&transaction).unwrap();

    println!("tx signature: {}", tx_signature);
}

#[derive(BorshSerialize)]
struct MovieReviewPayload {
    discriminator: u8,
    title: String,
    rating: u8,
    description: String
}