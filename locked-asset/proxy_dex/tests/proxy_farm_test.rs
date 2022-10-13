mod proxy_dex_test_setup;

use common_structs::FarmTokenAttributes;
use elrond_wasm::{elrond_codec::Empty, types::EsdtTokenPayment};
use elrond_wasm_debug::{
    managed_address, managed_biguint, managed_token_id, rust_biguint, tx_mock::TxInputESDT,
    DebugApi,
};
use proxy_dex::{
    proxy_farm::ProxyFarmModule, wrapped_farm_attributes::WrappedFarmTokenAttributes,
    wrapped_farm_token_merge::WrappedFarmTokenMerge,
};
use proxy_dex_test_setup::*;

#[test]
fn farm_proxy_actions_test() {
    let mut setup = ProxySetup::new(
        proxy_dex::contract_obj,
        pair::contract_obj,
        farm::contract_obj,
        simple_lock_energy::contract_obj,
    );
    let first_user = setup.first_user.clone();
    let farm_addr = setup.farm_wrapper.address_ref().clone();

    //////////////////////////////////////////// ENTER FARM /////////////////////////////////////

    setup
        .b_mock
        .execute_esdt_transfer(
            &first_user,
            &setup.proxy_wrapper,
            LOCKED_TOKEN_ID,
            1,
            &rust_biguint!(USER_BALANCE),
            |sc| {
                sc.enter_farm_proxy_endpoint(managed_address!(&farm_addr));
            },
        )
        .assert_ok();

    // check user balance
    setup.b_mock.check_nft_balance::<Empty>(
        &first_user,
        LOCKED_TOKEN_ID,
        1,
        &rust_biguint!(0),
        None,
    );
    setup.b_mock.check_nft_balance(
        &first_user,
        WRAPPED_FARM_TOKEN_ID,
        1,
        &rust_biguint!(USER_BALANCE),
        Some(&WrappedFarmTokenAttributes::<DebugApi> {
            proxy_farming_token: EsdtTokenPayment {
                token_identifier: managed_token_id!(LOCKED_TOKEN_ID),
                token_nonce: 1,
                amount: managed_biguint!(USER_BALANCE),
            },
            farm_token: EsdtTokenPayment {
                token_identifier: managed_token_id!(FARM_TOKEN_ID),
                token_nonce: 1,
                amount: managed_biguint!(USER_BALANCE),
            },
        }),
    );

    // check proxy balance
    setup
        .b_mock
        .check_nft_balance::<FarmTokenAttributes<DebugApi>>(
            setup.proxy_wrapper.address_ref(),
            FARM_TOKEN_ID,
            1,
            &rust_biguint!(USER_BALANCE),
            None,
        );

    // check farm balance
    setup.b_mock.check_esdt_balance(
        setup.farm_wrapper.address_ref(),
        MEX_TOKEN_ID,
        &rust_biguint!(USER_BALANCE),
    );

    setup.b_mock.set_block_epoch(50);
    setup.b_mock.set_block_nonce(100);

    //////////////////////////////////////////// CLAIM REWARDS /////////////////////////////////////

    // claim rewards with half position
    setup
        .b_mock
        .execute_esdt_transfer(
            &first_user,
            &setup.proxy_wrapper,
            WRAPPED_FARM_TOKEN_ID,
            1,
            &rust_biguint!(USER_BALANCE / 2),
            |sc| {
                sc.claim_rewards_proxy(managed_address!(&farm_addr));
            },
        )
        .assert_ok();

    // check user balance
    setup.b_mock.check_esdt_balance(
        &first_user,
        MEX_TOKEN_ID,
        &(rust_biguint!(PER_BLOCK_REWARD_AMOUNT) * 100u32 / 2u32),
    );
    setup.b_mock.check_nft_balance::<Empty>(
        &first_user,
        LOCKED_TOKEN_ID,
        1,
        &rust_biguint!(0),
        None,
    );
    // remaining old NFT
    setup.b_mock.check_nft_balance(
        &first_user,
        WRAPPED_FARM_TOKEN_ID,
        1,
        &rust_biguint!(USER_BALANCE / 2),
        Some(&WrappedFarmTokenAttributes::<DebugApi> {
            proxy_farming_token: EsdtTokenPayment {
                token_identifier: managed_token_id!(LOCKED_TOKEN_ID),
                token_nonce: 1,
                amount: managed_biguint!(USER_BALANCE),
            },
            farm_token: EsdtTokenPayment {
                token_identifier: managed_token_id!(FARM_TOKEN_ID),
                token_nonce: 1,
                amount: managed_biguint!(USER_BALANCE),
            },
        }),
    );
    // new NFT
    setup.b_mock.check_nft_balance(
        &first_user,
        WRAPPED_FARM_TOKEN_ID,
        2,
        &rust_biguint!(USER_BALANCE / 2),
        Some(&WrappedFarmTokenAttributes::<DebugApi> {
            proxy_farming_token: EsdtTokenPayment {
                token_identifier: managed_token_id!(LOCKED_TOKEN_ID),
                token_nonce: 1,
                amount: managed_biguint!(USER_BALANCE / 2),
            },
            farm_token: EsdtTokenPayment {
                token_identifier: managed_token_id!(FARM_TOKEN_ID),
                token_nonce: 2,
                amount: managed_biguint!(USER_BALANCE / 2),
            },
        }),
    );

    //////////////////////////////////////////// MERGE TOKENS /////////////////////////////////////

    let payments = vec![
        TxInputESDT {
            token_identifier: WRAPPED_FARM_TOKEN_ID.to_vec(),
            nonce: 1,
            value: rust_biguint!(USER_BALANCE / 2),
        },
        TxInputESDT {
            token_identifier: WRAPPED_FARM_TOKEN_ID.to_vec(),
            nonce: 2,
            value: rust_biguint!(USER_BALANCE / 2),
        },
    ];
    setup
        .b_mock
        .execute_esdt_multi_transfer(&first_user, &setup.proxy_wrapper, &payments, |sc| {
            sc.merge_wrapped_farm_tokens_endpoint(managed_address!(&farm_addr));
        })
        .assert_ok();

    // check user balance
    setup.b_mock.check_nft_balance(
        &first_user,
        WRAPPED_FARM_TOKEN_ID,
        3,
        &rust_biguint!(USER_BALANCE),
        Some(&WrappedFarmTokenAttributes::<DebugApi> {
            proxy_farming_token: EsdtTokenPayment {
                token_identifier: managed_token_id!(LOCKED_TOKEN_ID),
                token_nonce: 3,
                amount: managed_biguint!(USER_BALANCE),
            },
            farm_token: EsdtTokenPayment {
                token_identifier: managed_token_id!(FARM_TOKEN_ID),
                token_nonce: 3,
                amount: managed_biguint!(USER_BALANCE),
            },
        }),
    );
}