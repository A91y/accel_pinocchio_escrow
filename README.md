# accel-pinocchio-escrow

Token escrow on Solana using [pinocchio](https://github.com/anza-xyz/pinocchio).

## Instructions

| # | Name | Signer | Description |
|---|------|--------|-------------|
| 0 | `Make` | maker | Locks `amount_to_give` of mint_a in a vault, records `amount_to_receive` of mint_b |
| 1 | `Take` | taker | Sends mint_b to maker, receives mint_a from vault, closes vault + escrow |
| 2 | `Refund` | maker | Returns mint_a from vault to maker, closes vault + escrow |

## Accounts

**Make** — `[maker, mint_a, mint_b, escrow_pda, maker_ata_a, vault, system_program, token_program, ata_program]`

**Take** — `[taker, maker, mint_a, mint_b, taker_ata_a, taker_ata_b, maker_ata_b, escrow_pda, vault, token_program]`

**Refund** — `[maker, mint_a, maker_ata_a, escrow_pda, vault, token_program]`

## Escrow PDA

Seeds: `["escrow", maker_pubkey, bump]`

## Build & Test

```sh
cargo build-sbf
cargo test -- --nocapture
```
