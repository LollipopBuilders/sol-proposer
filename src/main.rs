//! A bridge service that reads state from L2 and submits to L1.

use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair},
    signer::Signer,
    transaction::Transaction,
    instruction::{AccountMeta, Instruction},
};
use eyre::Result;
use serde::Deserialize;
use std::str::FromStr;
use tokio::time::{interval, Duration};
use std::path::Path;

/// Configuration structure for the bridge service
#[derive(Debug, Deserialize)]
struct Config {
    network: NetworkConfig,
    account: AccountConfig,
    wallet: WalletConfig,
    settings: SettingsConfig,
}

/// Network-related configuration
#[derive(Debug, Deserialize)]
struct NetworkConfig {
    l1_rpc_url: String,
    l2_rpc_url: String,
    l1_program_id: String,
}

/// Account addresses configuration
#[derive(Debug, Deserialize)]
struct AccountConfig {
    leaf_chunk_address: String,
    slots_account: String,
}

/// Wallet configuration
#[derive(Debug, Deserialize)]
struct WalletConfig {
    wallet_path: String,
}

/// General settings configuration
#[derive(Debug, Deserialize)]
struct SettingsConfig {
    check_interval_secs: u64,
}

/// Loads configuration from config.toml file
fn load_config() -> Result<Config> {
    let settings = config::Config::builder()
        .add_source(config::File::with_name("config"))
        .build()?;
    
    Ok(settings.try_deserialize()?)
}

/// Loads wallet keypair from the specified path
async fn load_wallet(wallet_path: &str) -> Result<Keypair> {
    let expanded_path = shellexpand::tilde(wallet_path);
    let wallet_path = Path::new(expanded_path.as_ref());
    let keypair = read_keypair_file(wallet_path)
        .map_err(|e| eyre::eyre!("Failed to read wallet file: {}", e))?;
    Ok(keypair)
}

/// Main function to check L2 state and submit to L1
async fn check_and_submit(config: &Config) -> Result<()> {
    // Initialize RPC clients
    let l2_client = RpcClient::new_with_commitment(
        config.network.l2_rpc_url.clone(),
        CommitmentConfig::confirmed(),
    );
    let l1_client = RpcClient::new_with_commitment(
        config.network.l1_rpc_url.clone(),
        CommitmentConfig::confirmed(),
    );
    
    // Load wallet
    let wallet = load_wallet(&config.wallet.wallet_path).await?;
    
    // Get L2 account data and corresponding slot
    let leaf_chunk_pubkey = Pubkey::from_str(&config.account.leaf_chunk_address)?;
    let response = l2_client.get_account_with_commitment(
        &leaf_chunk_pubkey,
        CommitmentConfig::confirmed(),
    )?;
    
    let account = response.value.ok_or_else(|| {
        eyre::eyre!("Account not found")
    })?;
    
    let account_slot = response.context.slot;
    
    // Extract merkle tree root from account data
    let account_data = account.data;
    let mut mt_root = [0u8; 32];
    mt_root.copy_from_slice(&account_data[8..40]);
    // Use zero bytes for world state root (placeholder)
    let ws_root = [0u8; 32];
    
    println!("Merkle tree root from L2: 0x{}", hex::encode(mt_root));
    println!("World state root (fixed): 0x{}", hex::encode(ws_root));
    println!("Current slot: {}", account_slot);
    
    // Get program and account addresses
    let l1_program_id = Pubkey::from_str(&config.network.l1_program_id)?;
    let slots_account = Pubkey::from_str(&config.account.slots_account)?;
    
    // Calculate PDA for slot roots account
    let roots_seeds = &[b"roots".as_ref(), &account_slot.to_le_bytes()];
    let (slot_roots_account, _) = Pubkey::find_program_address(roots_seeds, &l1_program_id);
    
    // Create instruction data
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(&[249, 209, 47, 60, 18, 3, 81, 219]); // Instruction discriminator
    instruction_data.extend_from_slice(&account_slot.to_le_bytes());
    instruction_data.extend_from_slice(&mt_root);
    instruction_data.extend_from_slice(&ws_root);
    
    // Create and send transaction
    let instruction = Instruction::new_with_bytes(
        l1_program_id,
        &instruction_data,
        vec![
            AccountMeta::new(slots_account, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(slot_roots_account, false),
            AccountMeta::new(wallet.pubkey(), true),
        ],
    );
    
    let recent_blockhash = l1_client.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&wallet.pubkey()),
        &[&wallet],
        recent_blockhash,
    );
    
    let signature = l1_client.send_and_confirm_transaction(&transaction)?;
    println!("Transaction confirmed: {}", signature);
    
    Ok(())
}

/// Retry mechanism for async operations
async fn with_retry<F, Fut, T>(f: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut retries = 3;
    let mut last_error = None;
    
    while retries > 0 {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                retries -= 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    
    Err(last_error.unwrap())
}

/// Entry point of the bridge service
#[tokio::main]
async fn main() -> Result<()> {
    let config = load_config()?;
    let mut interval = interval(Duration::from_secs(config.settings.check_interval_secs));
    
    loop {
        interval.tick().await;
        if let Err(e) = check_and_submit(&config).await {
            eprintln!("Error: {}", e);
        }
    }
} 