use elrond_wasm::{storage::mappers::StorageTokenWrapper, types::EsdtLocalRole};
use elrond_wasm_debug::{
    managed_address, managed_biguint, managed_token_id, managed_token_id_wrapped, rust_biguint,
    testing_framework::BlockchainStateWrapper, DebugApi,
};
use energy_factory_mock::EnergyFactoryMock;
use energy_query::Energy;
use locked_token_wrapper::{
    wrapped_token::{WrappedTokenAttributes, WrappedTokenModule},
    LockedTokenWrapper,
};
use simple_lock::locked_token::LockedTokenAttributes;

static BASE_ASSET_TOKEN_ID: &[u8] = b"FREEEE-123456";
static LOCKED_TOKEN_ID: &[u8] = b"LOCKED-123456";
static LEGACY_LOCKED_TOKEN_ID: &[u8] = b"LEGACY-123456";
static WRAPPED_TOKEN_ID: &[u8] = b"WRAPPED-123456";

#[test]
fn token_wrap_unwrap_test() {
    let _ = DebugApi::dummy();
    let rust_zero = rust_biguint!(0);

    let mut b_mock = BlockchainStateWrapper::new();
    let owner = b_mock.create_user_account(&rust_zero);
    let first_user = b_mock.create_user_account(&rust_zero);
    let second_user = b_mock.create_user_account(&rust_zero);
    let energy_factory = b_mock.create_sc_account(
        &rust_zero,
        Some(&owner),
        energy_factory_mock::contract_obj,
        "energy factory mock",
    );
    let locked_token_wrapper = b_mock.create_sc_account(
        &rust_zero,
        Some(&owner),
        locked_token_wrapper::contract_obj,
        "locked token wrapper",
    );

    // setup wrapping SC
    b_mock
        .execute_tx(&owner, &locked_token_wrapper, &rust_zero, |sc| {
            sc.init(
                managed_token_id!(LEGACY_LOCKED_TOKEN_ID),
                managed_token_id!(LOCKED_TOKEN_ID),
                managed_address!(energy_factory.address_ref()),
            );

            sc.wrapped_token()
                .set_token_id(managed_token_id!(WRAPPED_TOKEN_ID));
        })
        .assert_ok();

    b_mock.set_esdt_local_roles(
        locked_token_wrapper.address_ref(),
        WRAPPED_TOKEN_ID,
        &[
            EsdtLocalRole::NftCreate,
            EsdtLocalRole::NftAddQuantity,
            EsdtLocalRole::NftBurn,
        ],
    );

    // simulate first user lock - 1_000 tokens for 20 epochs
    b_mock.set_nft_balance(
        &first_user,
        LOCKED_TOKEN_ID,
        1,
        &rust_biguint!(1_000),
        &LockedTokenAttributes::<DebugApi> {
            original_token_id: managed_token_id_wrapped!(BASE_ASSET_TOKEN_ID),
            original_token_nonce: 0,
            unlock_epoch: 20,
        },
    );

    b_mock
        .execute_tx(&owner, &energy_factory, &rust_zero, |sc| {
            let energy = Energy::new(
                (managed_biguint!(1_000) * 20u64).into(),
                0,
                managed_biguint!(1_000),
            );
            sc.user_energy(&managed_address!(&first_user)).set(&energy);
        })
        .assert_ok();

    // wrap 500 tokens
    b_mock
        .execute_esdt_transfer(
            &first_user,
            &locked_token_wrapper,
            LOCKED_TOKEN_ID,
            1,
            &rust_biguint!(500),
            |sc| {
                let _ = sc.wrap_locked_token_endpoint();
            },
        )
        .assert_ok();

    b_mock.check_nft_balance(
        &first_user,
        WRAPPED_TOKEN_ID,
        1,
        &rust_biguint!(500),
        Some(&WrappedTokenAttributes::<DebugApi> {
            locked_token_id: managed_token_id!(LOCKED_TOKEN_ID),
            locked_token_nonce: 1,
        }),
    );

    // check energy after wrap
    b_mock
        .execute_query(&energy_factory, |sc| {
            let expected_energy = Energy::new(
                (managed_biguint!(500) * 20u64).into(),
                0,
                managed_biguint!(500),
            );
            let actual_energy = sc.user_energy(&managed_address!(&first_user)).get();
            assert_eq!(actual_energy, expected_energy);
        })
        .assert_ok();

    // simulate first user transfering wrapped tokens to second user
    b_mock.set_nft_balance(
        &second_user,
        WRAPPED_TOKEN_ID,
        1,
        &rust_biguint!(500),
        &WrappedTokenAttributes::<DebugApi> {
            locked_token_id: managed_token_id!(LOCKED_TOKEN_ID),
            locked_token_nonce: 1,
        },
    );

    // 5 epochs pass
    b_mock.set_block_epoch(5);

    // second user unwrap
    b_mock
        .execute_esdt_transfer(
            &second_user,
            &locked_token_wrapper,
            WRAPPED_TOKEN_ID,
            1,
            &rust_biguint!(500),
            |sc| {
                let _ = sc.unwrap_locked_token_endpoint();
            },
        )
        .assert_ok();

    b_mock.check_nft_balance(
        &second_user,
        LOCKED_TOKEN_ID,
        1,
        &rust_biguint!(500),
        Some(&LockedTokenAttributes::<DebugApi> {
            original_token_id: managed_token_id_wrapped!(BASE_ASSET_TOKEN_ID),
            original_token_nonce: 0,
            unlock_epoch: 20,
        }),
    );

    // check energy after unwrap
    b_mock
        .execute_query(&energy_factory, |sc| {
            let expected_energy = Energy::new(
                (managed_biguint!(500) * 15u64).into(),
                5,
                managed_biguint!(500),
            );
            let actual_energy = sc.user_energy(&managed_address!(&second_user)).get();
            assert_eq!(actual_energy, expected_energy);
        })
        .assert_ok();
}
