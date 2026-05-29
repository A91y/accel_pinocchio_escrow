use pinocchio::{
    AccountView, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
};

use crate::state::Escrow;

pub fn process_refund_instruction(accounts: &mut [AccountView], _data: &[u8]) -> ProgramResult {
    let [
        maker,
        mint_a,
        maker_ata_a,
        escrow_account,
        vault,
        _token_program @ ..,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !maker.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let (bump, amount_to_give) = {
        let escrow_state = Escrow::from_account_info(escrow_account)?;

        if escrow_state.maker() != maker.address() {
            return Err(ProgramError::IllegalOwner);
        }
        if escrow_state.mint_a() != mint_a.address() {
            return Err(ProgramError::InvalidAccountData);
        }

        (escrow_state.bump, escrow_state.amount_to_give())
    };

    let bump_bytes = [bump];
    let signer_seeds = [
        Seed::from(b"escrow"),
        Seed::from(maker.address().as_array()),
        Seed::from(bump_bytes.as_ref()),
    ];

    pinocchio_token::instructions::Transfer::new(vault, maker_ata_a, escrow_account, amount_to_give)
        .invoke_signed(&[Signer::from(&signer_seeds)])?;

    pinocchio_token::instructions::CloseAccount::new(vault, maker, escrow_account)
        .invoke_signed(&[Signer::from(&signer_seeds)])?;

    let escrow_lamports = escrow_account.lamports();
    maker.set_lamports(maker.lamports() + escrow_lamports);
    escrow_account.set_lamports(0);

    Ok(())
}
