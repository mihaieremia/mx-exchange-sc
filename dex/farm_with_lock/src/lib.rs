#![no_std]
#![allow(clippy::too_many_arguments)]
#![feature(exact_size_is_empty)]

pub mod contexts;
pub mod ctx_events;
pub mod custom_config;
pub mod custom_rewards;
pub mod errors;
pub mod farm_token_merge;
use crate::assert;

use common_structs::{FarmTokenAttributes, Nonce};
use config::State;
use contexts::exit_farm::ExitFarmContext;
use errors::*;
use farm_token::FarmToken;

use crate::contexts::base::*;

elrond_wasm::imports!();
elrond_wasm::derive_imports!();

use config::{
    DEFAULT_BURN_GAS_LIMIT, DEFAULT_MINUMUM_FARMING_EPOCHS, DEFAULT_PENALTY_PERCENT,
    DEFAULT_TRANSFER_EXEC_GAS_LIMIT, MAX_PENALTY_PERCENT,
};

type EnterFarmResultType<BigUint> = EsdtTokenPayment<BigUint>;
type CompoundRewardsResultType<BigUint> = EsdtTokenPayment<BigUint>;
type ClaimRewardsResultType<BigUint> =
    MultiResult2<EsdtTokenPayment<BigUint>, EsdtTokenPayment<BigUint>>;
type ExitFarmResultType<BigUint> =
    MultiResult2<EsdtTokenPayment<BigUint>, EsdtTokenPayment<BigUint>>;

#[elrond_wasm::contract]
pub trait Farm:
    custom_rewards::CustomRewardsModule
    + rewards::RewardsModule
    + custom_config::CustomConfigModule
    + config::ConfigModule
    + token_send::TokenSendModule
    + token_merge::TokenMergeModule
    + farm_token::FarmTokenModule
    + farm_token_merge::FarmTokenMergeModule
    + events::EventsModule
    + contexts::ctx_helper::CtxHelper
    + ctx_events::ContextEventsModule
{
    #[proxy]
    fn locked_asset_factory(&self, to: ManagedAddress) -> factory::Proxy<Self::Api>;

    #[init]
    fn init(
        &self,
        reward_token_id: TokenIdentifier,
        farming_token_id: TokenIdentifier,
        locked_asset_factory_address: ManagedAddress,
        division_safety_constant: BigUint,
        pair_contract_address: ManagedAddress,
    ) {
        assert!(self, reward_token_id.is_esdt(), ERROR_NOT_AN_ESDT);
        assert!(self, farming_token_id.is_esdt(), ERROR_NOT_AN_ESDT);
        assert!(self, division_safety_constant != 0u64, ERROR_ZERO_AMOUNT);
        let farm_token = self.farm_token_id().get();
        assert!(self, reward_token_id != farm_token, ERROR_SAME_TOKEN_IDS);
        assert!(self, farming_token_id != farm_token, ERROR_SAME_TOKEN_IDS);

        self.state().set(&State::Inactive);
        self.penalty_percent()
            .set_if_empty(&DEFAULT_PENALTY_PERCENT);
        self.minimum_farming_epochs()
            .set_if_empty(&DEFAULT_MINUMUM_FARMING_EPOCHS);
        self.transfer_exec_gas_limit()
            .set_if_empty(&DEFAULT_TRANSFER_EXEC_GAS_LIMIT);
        self.burn_gas_limit().set_if_empty(&DEFAULT_BURN_GAS_LIMIT);
        self.division_safety_constant()
            .set_if_empty(&division_safety_constant);

        self.owner().set(&self.blockchain().get_caller());
        self.reward_token_id().set(&reward_token_id);
        self.farming_token_id().set(&farming_token_id);
        self.locked_asset_factory_address()
            .set(&locked_asset_factory_address);
        self.pair_contract_address().set(&pair_contract_address);
    }

    #[payable("*")]
    #[endpoint(enterFarm)]
    fn enter_farm(
        &self,
        #[var_args] opt_accept_funds_func: OptionalArg<ManagedBuffer>,
    ) -> EnterFarmResultType<Self::Api> {
        let mut context = self.new_enter_farm_context(opt_accept_funds_func);

        self.load_state(&mut context);
        assert!(
            self,
            context.get_contract_state() == &State::Active,
            ERROR_NOT_ACTIVE
        );

        self.load_farm_token_id(&mut context);
        assert!(
            self,
            !context.get_farm_token_id().is_empty(),
            ERROR_NO_FARM_TOKEN,
        );

        self.load_farming_token_id(&mut context);
        assert!(self, context.is_accepted_payment(), ERROR_BAD_PAYMENTS,);

        self.load_reward_token_id(&mut context);
        self.load_block_nonce(&mut context);
        self.load_block_epoch(&mut context);
        self.load_reward_per_share(&mut context);
        self.load_farm_token_supply(&mut context);
        self.load_division_safety_constant(&mut context);
        self.generate_aggregated_rewards(&mut context);

        let first_payment_amount = context
            .get_tx_input()
            .get_payments()
            .get_first()
            .amount
            .clone();

        let virtual_position = FarmToken {
            token_amount: self.create_payment(
                context.get_farm_token_id(),
                0,
                &first_payment_amount,
            ),
            attributes: FarmTokenAttributes {
                reward_per_share: context.get_reward_per_share().clone(),
                entering_epoch: context.get_block_epoch(),
                original_entering_epoch: context.get_block_epoch(),
                initial_farming_amount: first_payment_amount.clone(),
                compounded_reward: BigUint::zero(),
                current_farm_amount: first_payment_amount.clone(),
            },
        };

        let (new_farm_token, created_with_merge) = self.create_farm_tokens_by_merging(
            &virtual_position,
            context
                .get_tx_input()
                .get_payments()
                .get_additional()
                .unwrap(),
            context.get_storage_cache(),
        );
        context.set_output_position(new_farm_token, created_with_merge);

        self.commit_changes(&context);
        self.execute_output_payments(&context);
        self.emit_enter_farm_event_context(&context);

        context
            .get_output_payments()
            .get(0)
            .as_ref()
            .unwrap()
            .clone()
    }

    #[payable("*")]
    #[endpoint(exitFarm)]
    fn exit_farm(
        &self,
        #[payment_token] _payment_token_id: TokenIdentifier,
        #[payment_nonce] _token_nonce: Nonce,
        #[payment_amount] _amount: BigUint,
        #[var_args] opt_accept_funds_func: OptionalArg<ManagedBuffer>,
    ) -> ExitFarmResultType<Self::Api> {
        let mut context = self.new_exit_farm_context(opt_accept_funds_func);

        self.load_state(&mut context);
        assert!(
            self,
            context.get_contract_state() == &State::Active,
            ERROR_NOT_ACTIVE
        );

        self.load_farm_token_id(&mut context);
        assert!(
            self,
            !context.get_farm_token_id().is_empty(),
            ERROR_NO_FARM_TOKEN,
        );

        self.load_farming_token_id(&mut context);
        assert!(self, context.is_accepted_payment(), ERROR_BAD_PAYMENTS,);

        self.load_reward_token_id(&mut context);
        self.load_block_nonce(&mut context);
        self.load_block_epoch(&mut context);
        self.load_reward_per_share(&mut context);
        self.load_farm_token_supply(&mut context);
        self.load_division_safety_constant(&mut context);
        self.generate_aggregated_rewards(&mut context);
        self.load_farm_attributes(&mut context);

        self.generate_aggregated_rewards(&mut context);
        self.calculate_reward(&mut context);
        context.decrease_reward_reserve();
        self.calculate_initial_farming_amount(&mut context);
        self.increase_reward_with_compounded_rewards(&mut context);

        self.burn_penalty(&mut context);
        self.burn_position(&context);
        self.commit_changes(&context);

        self.send_rewards(&mut context);
        self.construct_output_payments_exit(&mut context);
        self.execute_output_payments(&context);
        self.emit_exit_farm_event_context(&context);

        self.construct_and_get_result(&context)
    }

    #[payable("*")]
    #[endpoint(claimRewards)]
    fn claim_rewards(
        &self,
        #[var_args] opt_accept_funds_func: OptionalArg<ManagedBuffer>,
    ) -> ClaimRewardsResultType<Self::Api> {
        let context = self.new_claim_rewards_context(opt_accept_funds_func);

        self.load_state(&mut context);
        assert!(
            self,
            context.get_contract_state() == &State::Active,
            ERROR_NOT_ACTIVE
        );

        self.load_farm_token_id(&mut context);
        assert!(
            self,
            !context.get_farm_token_id().is_empty(),
            ERROR_NO_FARM_TOKEN,
        );

        self.load_farming_token_id(&mut context);
        assert!(self, context.is_accepted_payment(), ERROR_BAD_PAYMENTS,);

        self.load_reward_token_id(&mut context);
        self.load_block_nonce(&mut context);
        self.load_block_epoch(&mut context);
        self.load_reward_per_share(&mut context);
        self.load_farm_token_supply(&mut context);
        self.load_division_safety_constant(&mut context);
        self.generate_aggregated_rewards(&mut context);
        self.load_farm_attributes(&mut context);

        self.generate_aggregated_rewards(&mut context);
        self.calculate_reward(&mut context);
        context.decrease_reward_reserve();

        self.calculate_initial_farming_amount(&mut context);
        let new_compound_reward_amount = self.calculate_new_compound_reward_amount(&context);

        let virtual_position = FarmToken {
            token_amount: EsdtTokenPayment::new(
                context.get_farm_token_id(),
                0,
                context
                    .get_tx_input()
                    .get_payments()
                    .get_first()
                    .amount
                    .clone(),
            ),
            attributes: FarmTokenAttributes {
                reward_per_share: context.get_reward_per_share(),
                entering_epoch: context.get_input_attributes().unwrap().entering_epoch,
                original_entering_epoch: context
                    .get_input_attributes()
                    .unwrap()
                    .original_entering_epoch,
                initial_farming_amount: context.get_initial_farming_amount().unwrap().clone(),
                compounded_reward: new_compound_reward_amount,
                current_farm_amount: context
                    .get_tx_input()
                    .get_payments()
                    .get_first()
                    .amount
                    .clone(),
            },
        };

        let (new_farm_token, created_with_merge) = self.create_farm_tokens_by_merging(
            &virtual_position,
            context
                .get_tx_input()
                .get_payments()
                .get_additional()
                .unwrap(),
            context.get_storage_cache(),
        );
        context.set_output_position(new_farm_token, created_with_merge);

        self.burn_position(&context);
        self.commit_changes(&context);

        self.send_rewards(&mut context);
        self.execute_output_payments(&context);
        self.emit_claim_rewards_event_context(&context);

        self.construct_and_get_result(&context)
    }

    #[payable("*")]
    #[endpoint(compoundRewards)]
    fn compound_rewards(
        &self,
        #[var_args] opt_accept_funds_func: OptionalArg<ManagedBuffer>,
    ) -> CompoundRewardsResultType<Self::Api> {
        let mut context = self.new_compound_rewards_context(opt_accept_funds_func);

        self.load_state(&mut context);
        assert!(
            self,
            context.get_contract_state() == &State::Active,
            ERROR_NOT_ACTIVE
        );

        self.load_farm_token_id(&mut context);
        assert!(
            self,
            !context.get_farm_token_id().is_empty(),
            ERROR_NO_FARM_TOKEN,
        );

        self.load_farming_token_id(&mut context);
        self.load_reward_token_id(&mut context);
        assert!(self, context.is_accepted_payment(), ERROR_BAD_PAYMENTS,);

        assert!(
            self,
            context.get_farming_token_id() == context.get_reward_token_id(),
            ERROR_DIFFERENT_TOKEN_IDS
        );

        self.load_block_nonce(&mut context);
        self.load_block_epoch(&mut context);
        self.load_reward_per_share(&mut context);
        self.load_farm_token_supply(&mut context);
        self.load_division_safety_constant(&mut context);
        self.generate_aggregated_rewards(&mut context);
        self.load_farm_attributes(&mut context);

        self.generate_aggregated_rewards(&mut context);
        self.calculate_reward(&mut context);
        context.decrease_reward_reserve();

        self.calculate_initial_farming_amount(&mut context);
        self.calculate_new_compound_reward_amount(&mut context);

        let virtual_position = FarmToken {
            token_amount: EsdtTokenPayment::new(
                context.get_farm_token_id().clone(),
                0,
                &context.get_tx_input().get_payments().get_first().amount
                    + context.get_position_reward().unwrap(),
            ),
            attributes: FarmTokenAttributes {
                reward_per_share: context.get_reward_per_share().clone(),
                entering_epoch: context.get_block_epoch(),
                original_entering_epoch: self.aggregated_original_entering_epoch_on_compound(
                    context.get_farm_token_id(),
                    &context.get_tx_input().get_payments().get_first().amount,
                    context.get_input_attributes(),
                    context.get_position_reward().unwrap(),
                ),
                initial_farming_amount: context.get_initial_farming_amount(),
                compounded_reward: self.calculate_new_compound_reward_amount(&context)
                    + context.get_position_reward().unwrap(),
                current_farm_amount: &context.get_tx_input().get_payments().get_first().amount
                    + context.get_position_reward().unwrap(),
            },
        };

        let (new_farm_token, created_with_merge) = self.create_farm_tokens_by_merging(
            &virtual_position,
            context
                .get_tx_input()
                .get_payments()
                .get_additional()
                .unwrap(),
            context.get_storage_cache(),
        );
        context.set_output_position(new_farm_token, created_with_merge);

        self.burn_position(&context);
        self.commit_changes(&context);

        self.execute_output_payments(&context);
        self.emit_compound_rewards_event_context(&context);

        context
            .get_output_payments()
            .get(0)
            .as_ref()
            .unwrap()
            .clone()
    }

    fn aggregated_original_entering_epoch_on_compound(
        &self,
        farm_token_id: &TokenIdentifier,
        position_amount: &BigUint,
        position_attributes: &FarmTokenAttributes<Self::Api>,
        reward_amount: &BigUint,
    ) -> u64 {
        if reward_amount == &0 {
            return position_attributes.original_entering_epoch;
        }

        let initial_position = FarmToken {
            token_amount: self.create_payment(farm_token_id, 0, position_amount),
            attributes: position_attributes.clone(),
        };

        let mut reward_position = initial_position.clone();
        reward_position.token_amount.amount = reward_amount.clone();
        reward_position.attributes.original_entering_epoch = self.blockchain().get_block_epoch();

        let mut items = ManagedVec::new();
        items.push(initial_position);
        items.push(reward_position);
        self.aggregated_original_entering_epoch(&items)
    }

    fn burn_farming_tokens(
        &self,
        farming_token_id: &TokenIdentifier,
        farming_amount: &BigUint,
        reward_token_id: &TokenIdentifier,
    ) {
        let pair_contract_address = self.pair_contract_address().get();
        if pair_contract_address.is_zero() {
            self.send()
                .esdt_local_burn(farming_token_id, 0, farming_amount);
        } else {
            let gas_limit = self.burn_gas_limit().get();
            self.pair_contract_proxy(pair_contract_address)
                .remove_liquidity_and_burn_token(
                    farming_token_id.clone(),
                    0,
                    farming_amount.clone(),
                    reward_token_id.clone(),
                )
                .with_gas_limit(gas_limit)
                .execute_on_dest_context_ignore_result();
        }
    }

    fn create_farm_tokens_by_merging(
        &self,
        virtual_position: &FarmToken<Self::Api>,
        additional_positions: &ManagedVec<EsdtTokenPayment<Self::Api>>,
        storage_cache: &StorageCache<Self::Api>,
    ) -> (FarmToken<Self::Api>, bool) {
        let additional_payments_len = additional_positions.len();
        let merged_attributes =
            self.get_merged_farm_token_attributes(additional_positions, Some(virtual_position));

        self.burn_farm_tokens_from_payments(additional_positions);

        let new_amount = merged_attributes.current_farm_amount.clone();
        let new_nonce = self.mint_farm_tokens(
            &storage_cache.farm_token_id,
            &new_amount,
            &merged_attributes,
        );

        let new_farm_token = FarmToken {
            token_amount: self.create_payment(&storage_cache.farm_token_id, new_nonce, &new_amount),
            attributes: merged_attributes,
        };
        let is_merged = additional_payments_len != 0;

        Ok((new_farm_token, is_merged))
    }

    fn send_back_farming_tokens(
        &self,
        farming_token_id: &TokenIdentifier,
        farming_amount: &BigUint,
        destination: &ManagedAddress,
        opt_accept_funds_func: &OptionalArg<ManagedBuffer>,
    ) {
        self.transfer_execute_custom(
            destination,
            farming_token_id,
            0,
            farming_amount,
            opt_accept_funds_func,
        )
        .unwrap_or_signal_error(self.type_manager());
    }

    fn send_rewards(&self, context: &mut dyn Context<Self::Api>) {
        if context.get_position_reward().unwrap() > &0u64 {
            let locked_asset_factory_address = self.locked_asset_factory_address().get();
            let result = self
                .locked_asset_factory(locked_asset_factory_address)
                .create_and_forward(
                    context.get_position_reward().clone(),
                    context.get_caller().clone(),
                    context.get_input_attributes().unwrap().entering_epoch,
                    context.get_opt_accept_funds_func().clone(),
                )
                .execute_on_dest_context_custom_range(|_, after| (after - 1, after));
            context.set_final_reward(result);
        } else {
            context.set_final_reward(self.create_payment(
                context.get_position_reward(),
                0,
                context.get_position_reward(),
            ));
        }
    }

    #[inline]
    fn should_apply_penalty(&self, entering_epoch: u64) -> bool {
        entering_epoch + self.minimum_farming_epochs().get() as u64
            > self.blockchain().get_block_epoch()
    }

    #[inline]
    fn get_penalty_amount(&self, amount: &BigUint) -> BigUint {
        amount * self.penalty_percent().get() / MAX_PENALTY_PERCENT
    }

    fn burn_penalty(&self, context: &mut ExitFarmContext<Self::Api>) {
        if self.should_apply_penalty(context.get_input_attributes().unwrap().entering_epoch) {
            let penalty_amount = self.get_penalty_amount(context.get_initial_farming_amount());
            if penalty_amount > 0u64 {
                self.burn_farming_tokens(
                    context.get_farming_token_id(),
                    &penalty_amount,
                    context.get_reward_token_id(),
                );
                context.decrease_farming_token_amount(&penalty_amount);
            }
        }
    }

    fn burn_position(&self, context: &dyn Context<Self::Api>) {
        let farm_token = context.get_tx_input().get_payments().get_first();
        self.burn_farm_tokens(
            &farm_token.token_identifier,
            farm_token.token_nonce,
            &farm_token.amount,
        );
    }

    fn calculate_new_compound_reward_amount(&self, context: &dyn Context<Self::Api>) -> BigUint {
        self.rule_of_three(
            &context.get_tx_input().get_payments().get_first().amount,
            &context.get_input_attributes().unwrap().current_farm_amount,
            &context.get_input_attributes().unwrap().compounded_reward,
        );
    }
}
