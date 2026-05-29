#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use litesvm::LiteSVM;
    use litesvm_token::{
        CreateAssociatedTokenAccount, CreateMint, MintTo,
        spl_token::{self},
    };
    use solana_instruction::{AccountMeta, Instruction};
    use solana_keypair::Keypair;
    use solana_message::Message;
    use solana_native_token::LAMPORTS_PER_SOL;
    use solana_pubkey::Pubkey;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

    const TOKEN_PROGRAM_ID: Pubkey = spl_token::ID;

    fn program_id() -> Pubkey {
        Pubkey::from(crate::ID)
    }

    fn so_path() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for subdir in &["sbpf-solana-solana", "sbf-solana-solana"] {
            let p = manifest_dir
                .join("target")
                .join(subdir)
                .join("release/accel_pinocchio_escrow.so");
            if p.exists() {
                return p;
            }
        }
        manifest_dir.join("target/deploy/accel_pinocchio_escrow.so")
    }

    fn setup() -> (LiteSVM, Keypair) {
        let mut svm = LiteSVM::new();
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL)
            .expect("Airdrop failed");

        let program_data = std::fs::read(so_path())
            .expect("Failed to read accel_pinocchio_escrow.so — run `cargo build-sbf` first");
        svm.add_program(program_id(), &program_data)
            .expect("Failed to add program");

        (svm, payer)
    }

    fn escrow_pda(maker: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"escrow", maker.as_ref()], &program_id())
    }

    fn ata_program() -> Pubkey {
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
            .parse()
            .unwrap()
    }

    fn system_program() -> Pubkey {
        solana_sdk_ids::system_program::ID
    }

    struct EscrowSetup {
        svm: LiteSVM,
        maker: Keypair,
        mint_a: Pubkey,
        mint_b: Pubkey,
        maker_ata_a: Pubkey,
        escrow: Pubkey,
        _escrow_bump: u8,
        vault: Pubkey,
        _amount_to_receive: u64,
        amount_to_give: u64,
    }

    fn setup_make(amount_to_receive: u64, amount_to_give: u64, mint_amount: u64) -> EscrowSetup {
        setup_make_with_discriminator(0, amount_to_receive, amount_to_give, mint_amount)
    }

    #[allow(dead_code)]
    fn setup_make_v2(amount_to_receive: u64, amount_to_give: u64, mint_amount: u64) -> EscrowSetup {
        setup_make_with_discriminator(3, amount_to_receive, amount_to_give, mint_amount)
    }

    fn setup_make_with_discriminator(
        discriminator: u8,
        amount_to_receive: u64,
        amount_to_give: u64,
        mint_amount: u64,
    ) -> EscrowSetup {
        let (mut svm, maker) = setup();

        let mint_a = CreateMint::new(&mut svm, &maker)
            .decimals(6)
            .authority(&maker.pubkey())
            .send()
            .unwrap();

        let mint_b = CreateMint::new(&mut svm, &maker)
            .decimals(6)
            .authority(&maker.pubkey())
            .send()
            .unwrap();

        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &maker, &mint_a)
            .owner(&maker.pubkey())
            .send()
            .unwrap();

        MintTo::new(&mut svm, &maker, &mint_a, &maker_ata_a, mint_amount)
            .send()
            .unwrap();

        let (escrow, escrow_bump) = escrow_pda(&maker.pubkey());
        let vault = spl_associated_token_account::get_associated_token_address(&escrow, &mint_a);

        let make_data = [
            vec![discriminator],
            vec![escrow_bump],
            amount_to_receive.to_le_bytes().to_vec(),
            amount_to_give.to_le_bytes().to_vec(),
        ]
        .concat();

        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(maker.pubkey(), true),
                AccountMeta::new(mint_a, false),
                AccountMeta::new(mint_b, false),
                AccountMeta::new(escrow, false),
                AccountMeta::new(maker_ata_a, false),
                AccountMeta::new(vault, false),
                AccountMeta::new_readonly(system_program(), false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
                AccountMeta::new_readonly(ata_program(), false),
            ],
            data: make_data,
        };

        let msg = Message::new(&[ix], Some(&maker.pubkey()));
        let blockhash = svm.latest_blockhash();
        let tx = Transaction::new(&[&maker], msg, blockhash);
        let meta = svm.send_transaction(tx).expect("Make instruction failed");
        println!("Make CU: {}", meta.compute_units_consumed);

        EscrowSetup {
            svm,
            maker,
            mint_a,
            mint_b,
            maker_ata_a,
            escrow,
            _escrow_bump: escrow_bump,
            vault,
            _amount_to_receive: amount_to_receive,
            amount_to_give,
        }
    }

    fn read_token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
        let account = svm.get_account(ata).expect("token account not found");
        let bytes: [u8; 8] = account.data[64..72].try_into().unwrap();
        u64::from_le_bytes(bytes)
    }

    fn account_is_closed(svm: &LiteSVM, pubkey: &Pubkey) -> bool {
        match svm.get_account(pubkey) {
            None => true,
            Some(acc) => acc.lamports == 0,
        }
    }

    #[test]
    fn test_make() {
        let s = setup_make(100_000_000, 500_000_000, 1_000_000_000);

        let escrow_account = s.svm.get_account(&s.escrow).expect("escrow not found");
        assert_eq!(escrow_account.owner, program_id());
        assert_eq!(escrow_account.data.len(), 113);

        let vault_balance = read_token_balance(&s.svm, &s.vault);
        assert_eq!(vault_balance, s.amount_to_give);

        let maker_balance = read_token_balance(&s.svm, &s.maker_ata_a);
        assert_eq!(maker_balance, 1_000_000_000 - s.amount_to_give);

        println!("test_make passed");
    }

    #[test]
    fn test_take() {
        let amount_to_receive = 100_000_000_u64;
        let amount_to_give = 500_000_000_u64;
        let mint_amount = 1_000_000_000_u64;

        let mut s = setup_make(amount_to_receive, amount_to_give, mint_amount);

        let taker = Keypair::new();
        s.svm
            .airdrop(&taker.pubkey(), 2 * LAMPORTS_PER_SOL)
            .unwrap();

        let taker_ata_b = CreateAssociatedTokenAccount::new(&mut s.svm, &taker, &s.mint_b)
            .owner(&taker.pubkey())
            .send()
            .unwrap();

        MintTo::new(&mut s.svm, &s.maker, &s.mint_b, &taker_ata_b, amount_to_receive)
            .send()
            .unwrap();

        let taker_ata_a = CreateAssociatedTokenAccount::new(&mut s.svm, &taker, &s.mint_a)
            .owner(&taker.pubkey())
            .send()
            .unwrap();

        let maker_ata_b = CreateAssociatedTokenAccount::new(&mut s.svm, &s.maker, &s.mint_b)
            .owner(&s.maker.pubkey())
            .send()
            .unwrap();

        let maker_lamports_before = s.svm.get_account(&s.maker.pubkey()).unwrap().lamports;

        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(taker.pubkey(), true),
                AccountMeta::new(s.maker.pubkey(), false),
                AccountMeta::new_readonly(s.mint_a, false),
                AccountMeta::new_readonly(s.mint_b, false),
                AccountMeta::new(taker_ata_a, false),
                AccountMeta::new(taker_ata_b, false),
                AccountMeta::new(maker_ata_b, false),
                AccountMeta::new(s.escrow, false),
                AccountMeta::new(s.vault, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
            ],
            data: vec![1u8],
        };

        let msg = Message::new(&[ix], Some(&taker.pubkey()));
        let blockhash = s.svm.latest_blockhash();
        let tx = Transaction::new(&[&taker], msg, blockhash);
        let meta = s.svm.send_transaction(tx).expect("Take instruction failed");
        println!("Take CU: {}", meta.compute_units_consumed);

        // taker received mint_a from vault
        assert_eq!(read_token_balance(&s.svm, &taker_ata_a), amount_to_give);

        // maker received mint_b from taker
        assert_eq!(read_token_balance(&s.svm, &maker_ata_b), amount_to_receive);

        // taker paid out all of their mint_b
        assert_eq!(read_token_balance(&s.svm, &taker_ata_b), 0);

        // vault and escrow are closed
        assert!(account_is_closed(&s.svm, &s.vault));
        assert!(account_is_closed(&s.svm, &s.escrow));

        // maker received rent lamports from vault + escrow close
        let maker_lamports_after = s.svm.get_account(&s.maker.pubkey()).unwrap().lamports;
        assert!(maker_lamports_after > maker_lamports_before);

        println!("test_take passed");
    }

    #[test]
    fn test_refund() {
        let amount_to_receive = 100_000_000_u64;
        let amount_to_give = 500_000_000_u64;
        let mint_amount = 1_000_000_000_u64;

        let mut s = setup_make(amount_to_receive, amount_to_give, mint_amount);

        let maker_ata_a_balance_before = read_token_balance(&s.svm, &s.maker_ata_a);
        let maker_lamports_before = s.svm.get_account(&s.maker.pubkey()).unwrap().lamports;

        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(s.maker.pubkey(), true),
                AccountMeta::new_readonly(s.mint_a, false),
                AccountMeta::new(s.maker_ata_a, false),
                AccountMeta::new(s.escrow, false),
                AccountMeta::new(s.vault, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
            ],
            data: vec![2u8],
        };

        let msg = Message::new(&[ix], Some(&s.maker.pubkey()));
        let blockhash = s.svm.latest_blockhash();
        let tx = Transaction::new(&[&s.maker], msg, blockhash);
        let meta = s
            .svm
            .send_transaction(tx)
            .expect("Refund instruction failed");
        println!("Refund CU: {}", meta.compute_units_consumed);

        // maker got back all of amount_to_give
        assert_eq!(
            read_token_balance(&s.svm, &s.maker_ata_a),
            maker_ata_a_balance_before + amount_to_give
        );

        // vault and escrow are closed
        assert!(account_is_closed(&s.svm, &s.vault));
        assert!(account_is_closed(&s.svm, &s.escrow));

        // maker recovered rent lamports from vault + escrow close (net positive after tx fee)
        let maker_lamports_after = s.svm.get_account(&s.maker.pubkey()).unwrap().lamports;
        assert!(maker_lamports_after > maker_lamports_before);

        println!("test_refund passed");
    }

    #[test]
    fn test_refund_not_maker_fails() {
        let mut s = setup_make(100_000_000, 500_000_000, 1_000_000_000);

        let impostor = Keypair::new();
        s.svm
            .airdrop(&impostor.pubkey(), 2 * LAMPORTS_PER_SOL)
            .unwrap();

        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(impostor.pubkey(), true),
                AccountMeta::new_readonly(s.mint_a, false),
                AccountMeta::new(s.maker_ata_a, false),
                AccountMeta::new(s.escrow, false),
                AccountMeta::new(s.vault, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
            ],
            data: vec![2u8],
        };

        let msg = Message::new(&[ix], Some(&impostor.pubkey()));
        let blockhash = s.svm.latest_blockhash();
        let tx = Transaction::new(&[&impostor], msg, blockhash);
        assert!(
            s.svm.send_transaction(tx).is_err(),
            "refund by non-maker should fail"
        );

        // escrow and vault must still be intact
        assert!(s.svm.get_account(&s.escrow).is_some());
        assert!(s.svm.get_account(&s.vault).is_some());

        println!("test_refund_not_maker_fails passed");
    }

    #[test]
    fn test_take_wrong_mint_b_fails() {
        let amount_to_receive = 100_000_000_u64;
        let amount_to_give = 500_000_000_u64;
        let mint_amount = 1_000_000_000_u64;

        let mut s = setup_make(amount_to_receive, amount_to_give, mint_amount);

        let taker = Keypair::new();
        s.svm
            .airdrop(&taker.pubkey(), 2 * LAMPORTS_PER_SOL)
            .unwrap();

        // Create a third mint that is NOT mint_b
        let fake_mint_b = CreateMint::new(&mut s.svm, &taker)
            .decimals(6)
            .authority(&taker.pubkey())
            .send()
            .unwrap();

        let taker_ata_fake_b =
            CreateAssociatedTokenAccount::new(&mut s.svm, &taker, &fake_mint_b)
                .owner(&taker.pubkey())
                .send()
                .unwrap();

        MintTo::new(
            &mut s.svm,
            &taker,
            &fake_mint_b,
            &taker_ata_fake_b,
            amount_to_receive,
        )
        .send()
        .unwrap();

        let taker_ata_a = CreateAssociatedTokenAccount::new(&mut s.svm, &taker, &s.mint_a)
            .owner(&taker.pubkey())
            .send()
            .unwrap();

        let maker_ata_fake_b =
            CreateAssociatedTokenAccount::new(&mut s.svm, &s.maker, &fake_mint_b)
                .owner(&s.maker.pubkey())
                .send()
                .unwrap();

        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(taker.pubkey(), true),
                AccountMeta::new(s.maker.pubkey(), false),
                AccountMeta::new_readonly(s.mint_a, false),
                AccountMeta::new_readonly(fake_mint_b, false), // wrong mint_b
                AccountMeta::new(taker_ata_a, false),
                AccountMeta::new(taker_ata_fake_b, false),
                AccountMeta::new(maker_ata_fake_b, false),
                AccountMeta::new(s.escrow, false),
                AccountMeta::new(s.vault, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
            ],
            data: vec![1u8],
        };

        let msg = Message::new(&[ix], Some(&taker.pubkey()));
        let blockhash = s.svm.latest_blockhash();
        let tx = Transaction::new(&[&taker], msg, blockhash);
        assert!(
            s.svm.send_transaction(tx).is_err(),
            "take with wrong mint_b should fail"
        );

        // escrow and vault must still be intact
        assert!(s.svm.get_account(&s.escrow).is_some());
        assert!(s.svm.get_account(&s.vault).is_some());

        println!("test_take_wrong_mint_b_fails passed");
    }
}
