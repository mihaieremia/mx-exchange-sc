elrond_wasm::imports!();
elrond_wasm::derive_imports!();

use crate::{
    error_messages::*,
    locked_token::{LockedTokenAttributes, PreviousStatusFlag},
};

#[derive(
    TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, PartialEq, Debug, Clone, Copy,
)]
pub enum FarmType {
    SimpleFarm,
    FarmWithLockedRewards,
}

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, PartialEq, Debug)]
pub struct FarmProxyTokenAttributes<M: ManagedTypeApi> {
    pub farm_type: FarmType,
    pub farm_token_id: TokenIdentifier<M>,
    pub farm_token_nonce: u64,
    pub farming_token_id: TokenIdentifier<M>,
    pub farming_token_locked_nonce: u64,
}

pub type EnterFarmThroughProxyResultType<M> = EsdtTokenPayment<M>;
pub type ExitFarmThroughProxyResultType<M> = MultiValue2<EsdtTokenPayment<M>, EsdtTokenPayment<M>>;
pub type FarmClaimRewardsThroughProxyResultType<M> =
    MultiValue2<EsdtTokenPayment<M>, EsdtTokenPayment<M>>;
pub type FarmCompoundRewardsThroughProxyResultType<M> = EsdtTokenPayment<M>;

#[elrond_wasm::module]
pub trait ProxyFarmModule:
    crate::farm_interactions::FarmInteractionsModule
    + crate::locked_token::LockedTokenModule
    + crate::token_attributes::TokenAttributesModule
    + elrond_wasm_modules::default_issue_callbacks::DefaultIssueCallbacksModule
{
    #[only_owner]
    #[payable("EGLD")]
    #[endpoint(issueFarmProxyToken)]
    fn issue_farm_proxy_token(
        &self,
        token_display_name: ManagedBuffer,
        token_ticker: ManagedBuffer,
        num_decimals: usize,
    ) {
        let payment_amount = self.call_value().egld_value();

        self.farm_proxy_token().issue(
            EsdtTokenType::Meta,
            payment_amount,
            token_display_name,
            token_ticker,
            num_decimals,
            None,
        );
    }

    #[only_owner]
    #[endpoint(setLocalRolesFarmProxyToken)]
    fn set_local_roles_farm_proxy_token(&self) {
        self.farm_proxy_token().set_local_roles(
            &[
                EsdtLocalRole::NftCreate,
                EsdtLocalRole::NftAddQuantity,
                EsdtLocalRole::NftBurn,
            ],
            None,
        );
    }

    #[only_owner]
    #[endpoint(addFarmToWhitelist)]
    fn add_farm_to_whitelist(
        &self,
        farm_address: ManagedAddress,
        farming_token_id: TokenIdentifier,
        farm_type: FarmType,
    ) {
        require!(
            self.blockchain().is_smart_contract(&farm_address),
            INVALID_SC_ADDRESS_ERR_MSG
        );

        self.farm_address_for_token(&farming_token_id, farm_type)
            .set(&farm_address);
    }

    #[only_owner]
    #[endpoint(removeFarmFromWhitelist)]
    fn remove_farm_from_whitelist(&self, farming_token_id: TokenIdentifier, farm_type: FarmType) {
        self.farm_address_for_token(&farming_token_id, farm_type)
            .clear();
    }

    #[payable("*")]
    #[endpoint(enterFarmLockedToken)]
    fn enter_farm_locked_token(
        &self,
        farm_type: FarmType,
    ) -> EnterFarmThroughProxyResultType<Self::Api> {
        let payment: EsdtTokenPayment<Self::Api> = self.call_value().payment();
        require!(payment.amount > 0, NO_PAYMENT_ERR_MSG);

        let locked_token_mapper = self.locked_token();
        locked_token_mapper.require_same_token(&payment.token_identifier);

        let locked_token_attributes: LockedTokenAttributes<Self::Api> =
            locked_token_mapper.get_token_attributes(payment.token_nonce);
        require!(
            locked_token_attributes.original_token_nonce == 0,
            ONLY_FUNGIBLE_TOKENS_ALLOWED_ERR_MSG
        );

        locked_token_mapper.nft_burn(payment.token_nonce, &payment.amount);

        let farm_address =
            self.try_get_farm_address(&locked_token_attributes.original_token_id, farm_type);
        let enter_farm_result = self.call_farm_enter(
            farm_address,
            locked_token_attributes.original_token_id.clone(),
            payment.amount,
        );
        let farm_tokens = enter_farm_result.farm_tokens;
        let proxy_farm_token_attributes = FarmProxyTokenAttributes {
            farm_type,
            farm_token_id: farm_tokens.token_identifier,
            farm_token_nonce: farm_tokens.token_nonce,
            farming_token_id: locked_token_attributes.original_token_id,
            farming_token_locked_nonce: payment.token_nonce,
        };

        let caller = self.blockchain().get_caller();
        self.farm_proxy_token().nft_create_and_send(
            &caller,
            farm_tokens.amount,
            &proxy_farm_token_attributes,
        )
    }

    #[payable("*")]
    #[endpoint(exitFarmLockedToken)]
    fn exit_farm_locked_token(&self) -> ExitFarmThroughProxyResultType<Self::Api> {
        let payment: EsdtTokenPayment<Self::Api> = self.call_value().payment();
        let farm_proxy_token_attributes: FarmProxyTokenAttributes<Self::Api> =
            self.validate_payment_and_get_farm_proxy_token_attributes(&payment);

        let farm_address = self.try_get_farm_address(
            &farm_proxy_token_attributes.farming_token_id,
            farm_proxy_token_attributes.farm_type,
        );
        let exit_farm_result = self.call_farm_exit(
            farm_address,
            farm_proxy_token_attributes.farm_token_id,
            farm_proxy_token_attributes.farm_token_nonce,
            payment.amount,
        );
        require!(
            exit_farm_result.initial_farming_tokens.token_identifier
                == farm_proxy_token_attributes.farming_token_id,
            INVALID_PAYMENTS_RECEIVED_FROM_FARM_ERR_MSG
        );

        let caller = self.blockchain().get_caller();
        let locked_tokens_payment = self.send_tokens_optimal_status(
            &caller,
            exit_farm_result.initial_farming_tokens,
            PreviousStatusFlag::Locked {
                locked_token_nonce: farm_proxy_token_attributes.farming_token_locked_nonce,
            },
        );

        if exit_farm_result.reward_tokens.amount > 0 {
            self.send().direct(
                &caller,
                &exit_farm_result.reward_tokens.token_identifier,
                exit_farm_result.reward_tokens.token_nonce,
                &exit_farm_result.reward_tokens.amount,
                &[],
            );
        }

        (locked_tokens_payment, exit_farm_result.reward_tokens).into()
    }

    #[payable("*")]
    #[endpoint(farmClaimRewardsLockedToken)]
    fn farm_claim_rewards_locked_token(&self) -> FarmClaimRewardsThroughProxyResultType<Self::Api> {
        let payment: EsdtTokenPayment<Self::Api> = self.call_value().payment();
        let mut farm_proxy_token_attributes: FarmProxyTokenAttributes<Self::Api> =
            self.validate_payment_and_get_farm_proxy_token_attributes(&payment);

        let farm_address = self.try_get_farm_address(
            &farm_proxy_token_attributes.farming_token_id,
            farm_proxy_token_attributes.farm_type,
        );
        let claim_rewards_result = self.call_farm_claim_rewards(
            farm_address,
            farm_proxy_token_attributes.farm_token_id.clone(),
            farm_proxy_token_attributes.farm_token_nonce,
            payment.amount,
        );
        require!(
            claim_rewards_result.new_farm_tokens.token_identifier
                == farm_proxy_token_attributes.farm_token_id,
            INVALID_PAYMENTS_RECEIVED_FROM_FARM_ERR_MSG
        );

        farm_proxy_token_attributes.farm_token_nonce =
            claim_rewards_result.new_farm_tokens.token_nonce;

        let caller = self.blockchain().get_caller();
        let new_proxy_token_payment = self.farm_proxy_token().nft_create_and_send(
            &caller,
            claim_rewards_result.new_farm_tokens.amount,
            &farm_proxy_token_attributes,
        );

        if claim_rewards_result.reward_tokens.amount > 0 {
            self.send().direct(
                &caller,
                &claim_rewards_result.reward_tokens.token_identifier,
                claim_rewards_result.reward_tokens.token_nonce,
                &claim_rewards_result.reward_tokens.amount,
                &[],
            );
        }

        (new_proxy_token_payment, claim_rewards_result.reward_tokens).into()
    }

    #[payable("*")]
    #[endpoint(farmCompoundRewardsLockedToken)]
    fn farm_compound_rewards_locked_token(
        &self,
    ) -> FarmCompoundRewardsThroughProxyResultType<Self::Api> {
        let payment: EsdtTokenPayment<Self::Api> = self.call_value().payment();
        let mut farm_proxy_token_attributes: FarmProxyTokenAttributes<Self::Api> =
            self.validate_payment_and_get_farm_proxy_token_attributes(&payment);

        let farm_address = self.try_get_farm_address(
            &farm_proxy_token_attributes.farming_token_id,
            farm_proxy_token_attributes.farm_type,
        );
        let compound_rewards_result = self.call_farm_compound_rewards(
            farm_address,
            farm_proxy_token_attributes.farm_token_id.clone(),
            farm_proxy_token_attributes.farm_token_nonce,
            payment.amount,
        );
        let new_farm_tokens = compound_rewards_result.new_farm_tokens;
        require!(
            new_farm_tokens.token_identifier == farm_proxy_token_attributes.farm_token_id,
            INVALID_PAYMENTS_RECEIVED_FROM_FARM_ERR_MSG
        );

        farm_proxy_token_attributes.farm_token_nonce = new_farm_tokens.token_nonce;

        let caller = self.blockchain().get_caller();
        self.farm_proxy_token().nft_create_and_send(
            &caller,
            new_farm_tokens.amount,
            &farm_proxy_token_attributes,
        )
    }

    fn try_get_farm_address(
        &self,
        farming_token_id: &TokenIdentifier,
        farm_type: FarmType,
    ) -> ManagedAddress {
        let mapper = self.farm_address_for_token(farming_token_id, farm_type);
        require!(
            !mapper.is_empty(),
            "No farm address for the specified token and type pair",
        );

        mapper.get()
    }

    fn validate_payment_and_get_farm_proxy_token_attributes(
        &self,
        payment: &EsdtTokenPayment<Self::Api>,
    ) -> FarmProxyTokenAttributes<Self::Api> {
        require!(payment.amount > 0, NO_PAYMENT_ERR_MSG);

        let farm_proxy_token_mapper = self.farm_proxy_token();
        farm_proxy_token_mapper.require_same_token(&payment.token_identifier);

        let farm_proxy_token_attributes: FarmProxyTokenAttributes<Self::Api> =
            farm_proxy_token_mapper.get_token_attributes(payment.token_nonce);

        farm_proxy_token_mapper.nft_burn(payment.token_nonce, &payment.amount);

        farm_proxy_token_attributes
    }

    #[storage_mapper("farmAddressForToken")]
    fn farm_address_for_token(
        &self,
        farming_token_id: &TokenIdentifier,
        farm_type: FarmType,
    ) -> SingleValueMapper<ManagedAddress>;

    #[view(getFarmProxyTokenId)]
    #[storage_mapper("farmProxyTokenId")]
    fn farm_proxy_token(&self) -> NonFungibleTokenMapper<Self::Api>;
}