# L2 State Bridge

A Rust service that bridges state from L2 to L1 on Solana blockchain. This service periodically reads the merkle tree root from a specified L2 account and submits it to L1 through a program call.

## Features

- Periodic state checking and submission
- Configurable check intervals
- Automatic retry mechanism
- Solana wallet integration
- Customizable RPC endpoints

## Prerequisites

- Rust 1.70 or higher
- Solana CLI tools
- A Solana wallet file

## Configuration

Create a `config.toml` file in the project root: 

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run --release
```

## Architecture

The service performs the following operations:

1. Reads configuration from `config.toml`
2. Connects to L2 network and reads the merkle tree root from the specified account
3. Calculates the PDA for storing roots on L1
4. Submits the state to L1 through a program transaction
5. Waits for the configured interval before the next check

## L1 Program Interface

The L1 program expects the following instruction format:

```rust
pub fn add_roots(
    ctx: Context<AddRoots>,
    slot: u64,
    mt_root: [u8; 32],
    ws_root: [u8; 32],
) -> Result<()>
```

## Error Handling

- Automatic retry mechanism for failed operations
- Detailed error logging
- Graceful error recovery