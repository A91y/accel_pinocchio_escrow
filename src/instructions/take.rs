use pinocchio::{
    AccountView, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
};

use crate::state::Escrow;

pub fn process_take_instruction(accounts: &mut [AccountView], _data: &[u8]) -> ProgramResult {
    let [
        taker,
        maker,
        mint_a,
        mint_b,
        taker_ata_a,
        taker_ata_b,
        maker_ata_b,
        escrow_account,
        vault,
        _token_program @ ..,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let (bump, amount_to_receive, amount_to_give, maker_key_bytes) = {
        let escrow_state = Escrow::from_account_info(escrow_account)?;

        if escrow_state.maker() != maker.address() {
            return Err(ProgramError::InvalidAccountData);
        }
        if escrow_state.mint_a() != mint_a.address() {
            return Err(ProgramError::InvalidAccountData);
        }
        if escrow_state.mint_b() != mint_b.address() {
            return Err(ProgramError::InvalidAccountData);
        }

        (
            escrow_state.bump,
            escrow_state.amount_to_receive(),
            escrow_state.amount_to_give(),
            *escrow_state.maker_raw(),
        )
    };

    let bump_bytes = [bump];
    let signer_seeds = [
        Seed::from(b"escrow"),
        Seed::from(maker_key_bytes.as_ref()),
        Seed::from(bump_bytes.as_ref()),
    ];

    pinocchio_token::instructions::Transfer::new(taker_ata_b, maker_ata_b, taker, amount_to_receive)
        .invoke()?;

    pinocchio_token::instructions::Transfer::new(vault, taker_ata_a, escrow_account, amount_to_give)
        .invoke_signed(&[Signer::from(&signer_seeds)])?;

    pinocchio_token::instructions::CloseAccount::new(vault, maker, escrow_account)
        .invoke_signed(&[Signer::from(&signer_seeds)])?;

    let escrow_lamports = escrow_account.lamports();
    maker.set_lamports(maker.lamports() + escrow_lamports);
    escrow_account.set_lamports(0);

    Ok(())
}
