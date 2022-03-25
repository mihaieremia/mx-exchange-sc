#![no_std]

use crate::{
    common_storage::MAX_PERCENTAGE,
    redeem_token::{ACCEPTED_TOKEN_REDEEM_NONCE, LAUNCHED_TOKEN_REDEEM_NONCE},
};

elrond_wasm::imports!();

pub mod common_storage;
pub mod events;
pub mod phase;
pub mod redeem_token;

const INVALID_PAYMENT_ERR_MSG: &[u8] = b"Invalid payment token";
const BELOW_MIN_PRICE_ERR_MSG: &[u8] = b"Launched token below min price";
pub const MIN_PRICE_PRECISION: u64 = 1_000_000_000_000_000_000;

#[elrond_wasm::contract]
pub trait PriceDiscovery:
    common_storage::CommonStorageModule
    + events::EventsModule
    + phase::PhaseModule
    + redeem_token::RedeemTokenModule
{
    /// For explanations regarding what each parameter means, please refer to docs/setup.md
    #[init]
    fn init(
        &self,
        launched_token_id: TokenIdentifier,
        accepted_token_id: TokenIdentifier,
        min_launched_token_price: BigUint,
        start_block: u64,
        no_limit_phase_duration_blocks: u64,
        linear_penalty_phase_duration_blocks: u64,
        fixed_penalty_phase_duration_blocks: u64,
        unbond_period_blocks: u64,
        penalty_min_percentage: BigUint,
        penalty_max_percentage: BigUint,
        fixed_penalty_percentage: BigUint,
    ) {
        /* Disabled until the validate token ID function is activated

        require!(
            launched_token_id.is_valid_esdt_identifier(),
            "Invalid launched token ID"
        );
        require!(
            accepted_token_id.is_egld() || accepted_token_id.is_valid_esdt_identifier(),
            "Invalid payment token ID"
        );
        require!(
            extra_rewards_token_id.is_egld() || extra_rewards_token_id.is_valid_esdt_identifier(),
            "Invalid extra rewards token ID"
        );

        */
        require!(
            launched_token_id != accepted_token_id,
            "Launched and accepted token must be different"
        );

        let current_block = self.blockchain().get_block_nonce();
        require!(
            current_block < start_block,
            "Start block cannot be in the past"
        );

        let end_block = start_block
            + no_limit_phase_duration_blocks
            + linear_penalty_phase_duration_blocks
            + fixed_penalty_phase_duration_blocks;

        require!(
            penalty_min_percentage <= penalty_max_percentage,
            "Min percentage higher than max percentage"
        );
        require!(
            penalty_max_percentage < MAX_PERCENTAGE,
            "Max percentage higher than 100%"
        );
        require!(
            fixed_penalty_percentage < MAX_PERCENTAGE,
            "Fixed percentage higher than 100%"
        );

        self.launched_token_id().set(&launched_token_id);
        self.accepted_token_id().set(&accepted_token_id);
        self.min_launched_token_price()
            .set(&min_launched_token_price);
        self.start_block().set(&start_block);
        self.end_block().set(&end_block);

        self.no_limit_phase_duration_blocks()
            .set(&no_limit_phase_duration_blocks);
        self.linear_penalty_phase_duration_blocks()
            .set(&linear_penalty_phase_duration_blocks);
        self.fixed_penalty_phase_duration_blocks()
            .set(&fixed_penalty_phase_duration_blocks);
        self.unbond_period_blocks().set(&unbond_period_blocks);
        self.penalty_min_percentage().set(&penalty_min_percentage);
        self.penalty_max_percentage().set(&penalty_max_percentage);
        self.fixed_penalty_percentage()
            .set(&fixed_penalty_percentage);
    }

    /// Users can deposit either launched_token or accepted_token.
    /// They will receive an SFT that can be used to withdraw said tokens
    #[payable("*")]
    #[endpoint]
    fn deposit(&self) {
        let phase = self.get_current_phase();
        self.require_deposit_allowed(&phase);

        let (payment_amount, payment_token) = self.call_value().payment_token_pair();
        let accepted_token_id = self.accepted_token_id().get();
        let launched_token_id = self.launched_token_id().get();
        let redeem_token_id = self.redeem_token_id().get();
        let (redeem_token_nonce, balance_mapper) = if payment_token == accepted_token_id {
            (ACCEPTED_TOKEN_REDEEM_NONCE, self.accepted_token_balance())
        } else if payment_token == launched_token_id {
            (LAUNCHED_TOKEN_REDEEM_NONCE, self.launched_token_balance())
        } else {
            sc_panic!(INVALID_PAYMENT_ERR_MSG);
        };

        let launched_token_balance_before = self.launched_token_balance().get();
        let accepted_token_balance_before = self.accepted_token_balance().get();

        self.increase_balance(balance_mapper, &payment_amount);

        let caller = self.blockchain().get_caller();
        self.mint_and_send_redeem_token(&caller, redeem_token_nonce, &payment_amount);

        let current_price = self.get_launched_token_price_over_min_price(
            &launched_token_balance_before,
            &accepted_token_balance_before,
        );

        self.emit_deposit_event(
            payment_token,
            payment_amount.clone(),
            redeem_token_id,
            redeem_token_nonce,
            payment_amount,
            current_price,
            phase,
        );
    }

    /// Deposit SFTs received after deposit to withdraw the initially deposited tokens.
    /// Depending on the current Phase, a penalty may be applied and only a part
    /// of the initial tokens will be received.
    #[payable("*")]
    #[endpoint]
    fn withdraw(&self) {
        let phase = self.get_current_phase();
        self.require_withdraw_allowed(&phase);

        let (payment_token, payment_nonce, payment_amount) = self.call_value().payment_as_tuple();
        let redeem_token_id = self.redeem_token_id().get();
        require!(payment_token == redeem_token_id, INVALID_PAYMENT_ERR_MSG);

        let (refund_token_id, balance_mapper) = match payment_nonce {
            LAUNCHED_TOKEN_REDEEM_NONCE => (
                self.launched_token_id().get(),
                self.launched_token_balance(),
            ),
            ACCEPTED_TOKEN_REDEEM_NONCE => (
                self.accepted_token_id().get(),
                self.accepted_token_balance(),
            ),
            _ => sc_panic!(INVALID_PAYMENT_ERR_MSG),
        };

        self.burn_redeem_token(payment_nonce, &payment_amount);

        let penalty_percentage = phase.get_penalty_percentage();
        let penalty_amount = &payment_amount * &penalty_percentage / MAX_PERCENTAGE;

        let launched_token_balance_before = self.launched_token_balance().get();
        let accepted_token_balance_before = self.accepted_token_balance().get();

        let caller = self.blockchain().get_caller();
        let withdraw_amount = &payment_amount - &penalty_amount;
        if withdraw_amount > 0 {
            self.decrease_balance(balance_mapper, &withdraw_amount);

            self.send()
                .direct(&caller, &refund_token_id, 0, &withdraw_amount, &[]);
        }

        let current_price = self.get_launched_token_price_over_min_price(
            &launched_token_balance_before,
            &accepted_token_balance_before,
        );

        self.emit_withdraw_event(
            refund_token_id,
            withdraw_amount,
            payment_token,
            payment_nonce,
            payment_amount,
            current_price,
            phase,
        );
    }

    /// After the unbond period has ended,
    /// users can withdraw their fair share of either accepted or launched tokens,
    /// depending on which token they deposited initially.
    /// Users that deposited accepted tokens receives launched tokens and vice-versa.
    #[payable("*")]
    #[endpoint]
    fn redeem(&self) {
        let phase = self.get_current_phase();
        self.require_redeem_allowed(&phase);

        let (payment_token, payment_nonce, payment_amount) = self.call_value().payment_as_tuple();
        let redeem_token_id = self.redeem_token_id().get();
        require!(payment_token == redeem_token_id, INVALID_PAYMENT_ERR_MSG);

        let bought_tokens = self.compute_bought_tokens(payment_nonce, &payment_amount);
        self.burn_redeem_token_without_supply_decrease(payment_nonce, &payment_amount);

        if bought_tokens.amount > 0 {
            let caller = self.blockchain().get_caller();
            self.send().direct(
                &caller,
                &bought_tokens.token_identifier,
                0,
                &bought_tokens.amount,
                &[],
            );
        }

        self.emit_redeem_event(
            payment_token,
            payment_nonce,
            payment_amount,
            bought_tokens.token_identifier,
            bought_tokens.amount,
        );
    }

    // private

    fn compute_bought_tokens(
        &self,
        redeem_token_nonce: u64,
        redeem_token_amount: &BigUint,
    ) -> EsdtTokenPayment<Self::Api> {
        let redeem_token_supply = self
            .redeem_token_total_circulating_supply(redeem_token_nonce)
            .get();

        // users that deposited accepted tokens get launched tokens, and vice-versa
        let (token_id, total_token_supply) = match redeem_token_nonce {
            ACCEPTED_TOKEN_REDEEM_NONCE => (
                self.launched_token_id().get(),
                self.launched_token_balance().get(),
            ),
            LAUNCHED_TOKEN_REDEEM_NONCE => (
                self.accepted_token_id().get(),
                self.accepted_token_balance().get(),
            ),
            _ => sc_panic!(INVALID_PAYMENT_ERR_MSG),
        };
        let reward_amount = total_token_supply * redeem_token_amount / redeem_token_supply;

        EsdtTokenPayment {
            token_type: EsdtTokenType::Fungible,
            token_identifier: token_id,
            token_nonce: 0,
            amount: reward_amount,
        }
    }

    fn get_launched_token_price_over_min_price(
        &self,
        launched_token_balance_before: &BigUint,
        accepted_token_balance_before: &BigUint,
    ) -> BigUint {
        let min_price = self.min_launched_token_price().get();
        let launched_token_balance_after = self.launched_token_balance().get();
        let accepted_token_balance_after = self.accepted_token_balance().get();

        if accepted_token_balance_after == 0 {
            return accepted_token_balance_after;
        }

        require!(
            launched_token_balance_after > 0,
            "No launched tokens available"
        );

        let price_before =
            self.calculate_price(accepted_token_balance_before, launched_token_balance_before);
        let price_after =
            self.calculate_price(&accepted_token_balance_after, &launched_token_balance_after);

        // If price is below min price before and after
        // it means there is a surplus of Launched tokens
        // so only Accepted token deposits are allowed
        if price_before < min_price && price_after < min_price {
            require!(
                &accepted_token_balance_after > accepted_token_balance_before,
                BELOW_MIN_PRICE_ERR_MSG
            );
        } else {
            require!(price_after >= min_price, BELOW_MIN_PRICE_ERR_MSG);
        }

        price_after
    }

    fn calculate_price(
        &self,
        accepted_token_balance: &BigUint,
        launched_token_balance: &BigUint,
    ) -> BigUint {
        if launched_token_balance == &0 {
            return BigUint::zero();
        }

        accepted_token_balance * MIN_PRICE_PRECISION / launched_token_balance
    }

    fn increase_balance(&self, mapper: SingleValueMapper<BigUint>, amount: &BigUint) {
        mapper.update(|b| *b += amount);
    }

    fn decrease_balance(&self, mapper: SingleValueMapper<BigUint>, amount: &BigUint) {
        mapper.update(|b| *b -= amount);
    }

    #[view(getMinLaunchedTokenPrice)]
    #[storage_mapper("minLaunchedTokenPrice")]
    fn min_launched_token_price(&self) -> SingleValueMapper<BigUint>;
}