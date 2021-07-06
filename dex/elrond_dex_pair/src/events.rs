elrond_wasm::imports!();
elrond_wasm::derive_imports!();

use dex_common::FftTokenAmountPair;

#[derive(TopEncode)]
pub struct SwapEvent<BigUint: BigUintApi> {
    sc_address: Address,
    user_address: Address,
    token_amount_in: FftTokenAmountPair<BigUint>,
    token_amount_out: FftTokenAmountPair<BigUint>,
    fee_amount: BigUint,
    block: u64,
    epoch: u64,
    timestamp: u64,
}

#[derive(TopEncode)]
pub struct AddLiquidityEvent<BigUint: BigUintApi> {
    sc_address: Address,
    user_address: Address,
    first_token_amount: FftTokenAmountPair<BigUint>,
    second_token_amount: FftTokenAmountPair<BigUint>,
    lp_token_amount: FftTokenAmountPair<BigUint>,
    block: u64,
    epoch: u64,
    timestamp: u64,
}

#[derive(TopEncode)]
pub struct RemoveLiquidityEvent<BigUint: BigUintApi> {
    sc_address: Address,
    user_address: Address,
    first_token_amount: FftTokenAmountPair<BigUint>,
    second_token_amount: FftTokenAmountPair<BigUint>,
    lp_token_amount: FftTokenAmountPair<BigUint>,
    block: u64,
    epoch: u64,
    timestamp: u64,
}

#[elrond_wasm_derive::module]
pub trait EventsModule {
    fn emit_swap_event(
        &self,
        user_address: &Address,
        token_amount_in: &FftTokenAmountPair<Self::BigUint>,
        token_amount_out: &FftTokenAmountPair<Self::BigUint>,
        fee_amount: &Self::BigUint,
    ) {
        let epoch = self.blockchain().get_block_epoch();
        self.swap_event(
            &token_amount_in.token_id,
            &token_amount_out.token_id,
            epoch,
            SwapEvent {
                sc_address: self.blockchain().get_sc_address(),
                user_address: user_address.clone(),
                token_amount_in: token_amount_in.clone(),
                token_amount_out: token_amount_out.clone(),
                fee_amount: fee_amount.clone(),
                block: self.blockchain().get_block_nonce(),
                epoch,
                timestamp: self.blockchain().get_block_timestamp(),
            },
        )
    }

    fn emit_add_liquidity_event(
        &self,
        user_address: &Address,
        first_token_amount: &FftTokenAmountPair<Self::BigUint>,
        second_token_amount: &FftTokenAmountPair<Self::BigUint>,
        lp_token_amount: &FftTokenAmountPair<Self::BigUint>,
    ) {
        let epoch = self.blockchain().get_block_epoch();
        self.add_liquidity_event(
            &first_token_amount.token_id,
            &second_token_amount.token_id,
            epoch,
            AddLiquidityEvent {
                sc_address: self.blockchain().get_sc_address(),
                user_address: user_address.clone(),
                first_token_amount: first_token_amount.clone(),
                second_token_amount: second_token_amount.clone(),
                lp_token_amount: lp_token_amount.clone(),
                block: self.blockchain().get_block_nonce(),
                epoch,
                timestamp: self.blockchain().get_block_timestamp(),
            },
        )
    }

    fn emit_remove_liquidity_event(
        &self,
        user_address: &Address,
        first_token_amount: &FftTokenAmountPair<Self::BigUint>,
        second_token_amount: &FftTokenAmountPair<Self::BigUint>,
        lp_token_amount: &FftTokenAmountPair<Self::BigUint>,
    ) {
        let epoch = self.blockchain().get_block_epoch();
        self.remove_liquidity_event(
            &first_token_amount.token_id,
            &second_token_amount.token_id,
            epoch,
            RemoveLiquidityEvent {
                sc_address: self.blockchain().get_sc_address(),
                user_address: user_address.clone(),
                first_token_amount: first_token_amount.clone(),
                second_token_amount: second_token_amount.clone(),
                lp_token_amount: lp_token_amount.clone(),
                block: self.blockchain().get_block_nonce(),
                epoch,
                timestamp: self.blockchain().get_block_timestamp(),
            },
        )
    }

    #[event("swap")]
    fn swap_event(
        &self,
        #[indexed] token_in: &TokenIdentifier,
        #[indexed] token_out: &TokenIdentifier,
        #[indexed] epoch: u64,
        swap_event: SwapEvent<Self::BigUint>,
    );

    #[event("add_liquidity")]
    fn add_liquidity_event(
        &self,
        #[indexed] first_token: &TokenIdentifier,
        #[indexed] second_token: &TokenIdentifier,
        #[indexed] epoch: u64,
        add_liquidity_event: AddLiquidityEvent<Self::BigUint>,
    );

    #[event("remove_liquidity")]
    fn remove_liquidity_event(
        &self,
        #[indexed] first_token: &TokenIdentifier,
        #[indexed] second_token: &TokenIdentifier,
        #[indexed] epoch: u64,
        remove_liquidity_event: RemoveLiquidityEvent<Self::BigUint>,
    );
}
