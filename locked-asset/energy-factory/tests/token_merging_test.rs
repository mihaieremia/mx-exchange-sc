mod energy_factory_setup;

use elrond_wasm::elrond_codec::multi_types::OptionalValue;
use elrond_wasm_debug::tx_mock::TxInputESDT;
use energy_factory::token_merging::TokenMergingModule;
use energy_factory_setup::*;
use simple_lock::locked_token::LockedTokenAttributes;

use elrond_wasm_debug::{managed_token_id_wrapped, rust_biguint, DebugApi};

#[test]
fn token_merging_test() {
    let _ = DebugApi::dummy();
    let mut setup = SimpleLockEnergySetup::new(energy_factory::contract_obj);
    let first_user = setup.first_user.clone();

    let first_token_amount = 400_000;
    let first_token_unlock_epoch = to_start_of_month(LOCK_OPTIONS[0]);
    setup
        .lock(
            &first_user,
            BASE_ASSET_TOKEN_ID,
            first_token_amount,
            LOCK_OPTIONS[0],
        )
        .assert_ok();

    let second_token_amount = 100_000;
    let second_token_unlock_epoch = to_start_of_month(LOCK_OPTIONS[1]);
    setup
        .lock(
            &first_user,
            BASE_ASSET_TOKEN_ID,
            second_token_amount,
            LOCK_OPTIONS[1],
        )
        .assert_ok();

    let payments = [
        TxInputESDT {
            token_identifier: LOCKED_TOKEN_ID.to_vec(),
            nonce: 1,
            value: rust_biguint!(400_000),
        },
        TxInputESDT {
            token_identifier: LOCKED_TOKEN_ID.to_vec(),
            nonce: 2,
            value: rust_biguint!(100_000),
        },
    ];
    setup
        .b_mock
        .execute_esdt_multi_transfer(&first_user, &setup.sc_wrapper, &payments[..], |sc| {
            let _ = sc.merge_tokens_endpoint(OptionalValue::None);
        })
        .assert_ok();

    assert_eq!(first_token_unlock_epoch, 360);
    assert_eq!(second_token_unlock_epoch, 720);

    // (400_000 * 360 + 100_000 * 720) / 500_000 = 4_400
    // 6_400 unlock fee -> epoch 432
    // -> start of month (upper) = 450
    let expected_merged_token_unlock_epoch = 450;
    setup.b_mock.check_nft_balance(
        &first_user,
        LOCKED_TOKEN_ID,
        3,
        &rust_biguint!(first_token_amount + second_token_amount),
        Some(&LockedTokenAttributes::<DebugApi> {
            original_token_id: managed_token_id_wrapped!(BASE_ASSET_TOKEN_ID),
            original_token_nonce: 0,
            unlock_epoch: expected_merged_token_unlock_epoch,
        }),
    );

    let expected_energy = rust_biguint!(500_000) * expected_merged_token_unlock_epoch;
    let actual_energy = setup.get_user_energy(&first_user);
    assert_eq!(expected_energy, actual_energy);
}

#[test]
fn token_merging_different_years_test() {
    let _ = DebugApi::dummy();
    let mut setup = SimpleLockEnergySetup::new(energy_factory::contract_obj);
    let first_user = setup.first_user.clone();

    let first_token_amount = 400_000;
    let first_token_unlock_epoch = to_start_of_month(LOCK_OPTIONS[1]);
    setup
        .lock(
            &first_user,
            BASE_ASSET_TOKEN_ID,
            first_token_amount,
            LOCK_OPTIONS[1],
        )
        .assert_ok();

    let second_token_amount = 100_000;
    let second_token_unlock_epoch = to_start_of_month(LOCK_OPTIONS[2]);
    setup
        .lock(
            &first_user,
            BASE_ASSET_TOKEN_ID,
            second_token_amount,
            LOCK_OPTIONS[2],
        )
        .assert_ok();

    let payments = [
        TxInputESDT {
            token_identifier: LOCKED_TOKEN_ID.to_vec(),
            nonce: 1,
            value: rust_biguint!(400_000),
        },
        TxInputESDT {
            token_identifier: LOCKED_TOKEN_ID.to_vec(),
            nonce: 2,
            value: rust_biguint!(100_000),
        },
    ];
    setup
        .b_mock
        .execute_esdt_multi_transfer(&first_user, &setup.sc_wrapper, &payments[..], |sc| {
            let _ = sc.merge_tokens_endpoint(OptionalValue::None);
        })
        .assert_ok();

    assert_eq!(first_token_unlock_epoch, 720);
    assert_eq!(second_token_unlock_epoch, 1440);

    // (400_000 * 6_000 + 100_000 * 8_000) / 500_000 = 6_400 (unlock fee)
    // 6_400 unlock fee -> epoch 864
    // -> start of month (upper) = 870
    let expected_merged_token_unlock_epoch = 870;
    setup.b_mock.check_nft_balance(
        &first_user,
        LOCKED_TOKEN_ID,
        3,
        &rust_biguint!(first_token_amount + second_token_amount),
        Some(&LockedTokenAttributes::<DebugApi> {
            original_token_id: managed_token_id_wrapped!(BASE_ASSET_TOKEN_ID),
            original_token_nonce: 0,
            unlock_epoch: expected_merged_token_unlock_epoch,
        }),
    );

    let expected_energy = rust_biguint!(500_000) * expected_merged_token_unlock_epoch;
    let actual_energy = setup.get_user_energy(&first_user);
    assert_eq!(expected_energy, actual_energy);
}

#[test]
fn token_merging_different_years2_test() {
    let _ = DebugApi::dummy();
    let mut setup = SimpleLockEnergySetup::new(energy_factory::contract_obj);
    let first_user = setup.first_user.clone();

    let first_token_amount = 400_000;
    let first_token_unlock_epoch = to_start_of_month(LOCK_OPTIONS[0]);
    setup
        .lock(
            &first_user,
            BASE_ASSET_TOKEN_ID,
            first_token_amount,
            LOCK_OPTIONS[0],
        )
        .assert_ok();

    let second_token_amount = 100_000;
    let second_token_unlock_epoch = to_start_of_month(LOCK_OPTIONS[2]);
    setup
        .lock(
            &first_user,
            BASE_ASSET_TOKEN_ID,
            second_token_amount,
            LOCK_OPTIONS[2],
        )
        .assert_ok();

    let payments = [
        TxInputESDT {
            token_identifier: LOCKED_TOKEN_ID.to_vec(),
            nonce: 1,
            value: rust_biguint!(400_000),
        },
        TxInputESDT {
            token_identifier: LOCKED_TOKEN_ID.to_vec(),
            nonce: 2,
            value: rust_biguint!(100_000),
        },
    ];
    setup
        .b_mock
        .execute_esdt_multi_transfer(&first_user, &setup.sc_wrapper, &payments[..], |sc| {
            let _ = sc.merge_tokens_endpoint(OptionalValue::None);
        })
        .assert_ok();

    assert_eq!(first_token_unlock_epoch, 360);
    assert_eq!(second_token_unlock_epoch, 1440);

    // (400_000 * 4_000 + 100_000 * 8_000) / 500_000 = 4_800
    // 4_800 unlock fee -> epoch 504
    // -> start of month (upper) = 510
    let expected_merged_token_unlock_epoch = 510;
    setup.b_mock.check_nft_balance(
        &first_user,
        LOCKED_TOKEN_ID,
        3,
        &rust_biguint!(first_token_amount + second_token_amount),
        Some(&LockedTokenAttributes::<DebugApi> {
            original_token_id: managed_token_id_wrapped!(BASE_ASSET_TOKEN_ID),
            original_token_nonce: 0,
            unlock_epoch: expected_merged_token_unlock_epoch,
        }),
    );

    let expected_energy = rust_biguint!(500_000) * expected_merged_token_unlock_epoch;
    let actual_energy = setup.get_user_energy(&first_user);
    assert_eq!(expected_energy, actual_energy);
}