multiversx_sc::imports!();

use common_errors::{ERROR_BAD_INPUT_TOKEN, ERROR_PARAMETERS};
use core::cmp::Ordering;

use crate::{
    amm, config,
    errors::{ERROR_SAFE_PRICE_OBSERVATION_DOES_NOT_EXIST, ERROR_ZERO_AMOUNT},
    safe_price::{self, PriceObservation, Round},
};

pub const DEFAULT_SAFE_PRICE_ROUNDS_OFFSET: u64 = 10 * 60 * 24;

#[multiversx_sc::module]
pub trait SafePriceViewModule:
    safe_price::SafePriceModule
    + config::ConfigModule
    + token_send::TokenSendModule
    + amm::AmmModule
    + permissions_module::PermissionsModule
    + pausable::PausableModule
{
    #[label("safe-price-view")]
    #[view(getTokensForGivenPositionWithSafePrice)]
    fn get_tokens_for_given_position_with_safe_price(
        &self,
        liquidity: BigUint,
    ) -> MultiValue2<EsdtTokenPayment<Self::Api>, EsdtTokenPayment<Self::Api>> {
        let lp_total_supply = self.lp_token_supply().get();
        let first_token_id = self.first_token_id().get();
        let second_token_id = self.second_token_id().get();
        if lp_total_supply == 0 {
            return MultiValue2::from((
                EsdtTokenPayment::new(first_token_id, 0, BigUint::zero()),
                EsdtTokenPayment::new(second_token_id, 0, BigUint::zero()),
            ));
        }

        let current_round = self.blockchain().get_block_round();
        let safe_price_current_index = self.safe_price_current_index().get();
        let safe_price_max_observations = self.safe_price_max_observations().get();
        let price_observations = self.price_observations().get();

        let last_price_observation = self.get_price_observation(
            safe_price_current_index,
            safe_price_max_observations,
            &price_observations,
            current_round,
        );

        let oldest_observation_index = if price_observations.len() < safe_price_max_observations {
            0
        } else {
            (safe_price_current_index + 1) % safe_price_max_observations
        };

        let oldest_observation = match price_observations.try_get(oldest_observation_index) {
            Some(obs) => obs,
            None => sc_panic!(ERROR_SAFE_PRICE_OBSERVATION_DOES_NOT_EXIST),
        };

        let mut first_price_observation = oldest_observation;
        if (last_price_observation.recording_round - first_price_observation.recording_round)
            > DEFAULT_SAFE_PRICE_ROUNDS_OFFSET
        {
            let offset_round =
                last_price_observation.recording_round - DEFAULT_SAFE_PRICE_ROUNDS_OFFSET;
            first_price_observation = self.get_price_observation(
                safe_price_current_index,
                safe_price_max_observations,
                &price_observations,
                offset_round,
            );
        }

        let first_token_reserve = self.pair_reserve(&first_token_id).get();
        let second_token_reserve = self.pair_reserve(&second_token_id).get();
        let first_token_worth = &liquidity * &first_token_reserve / &lp_total_supply;
        let second_token_worth = &liquidity * &second_token_reserve / &lp_total_supply;

        let first_token_payment = EsdtTokenPayment::new(first_token_id, 0, first_token_worth);
        let second_token_payment = EsdtTokenPayment::new(second_token_id, 0, second_token_worth);
        let first_token_weighted = self.compute_weighted_price(
            second_token_payment,
            &first_price_observation,
            &last_price_observation,
        );

        let second_token_weighted = self.compute_weighted_price(
            first_token_payment,
            &first_price_observation,
            &last_price_observation,
        );

        MultiValue2::from((first_token_weighted, second_token_weighted))
    }

    #[label("safe-price-view")]
    #[view(getSafePrice)]
    fn get_safe_price(
        &self,
        start_round: Round,
        end_round: Round,
        input_payment: EsdtTokenPayment<Self::Api>,
    ) -> EsdtTokenPayment<Self::Api> {
        require!(end_round >= start_round, ERROR_PARAMETERS);

        let safe_price_current_index = self.safe_price_current_index().get();
        let safe_price_max_observations = self.safe_price_max_observations().get();

        let price_observations = self.price_observations().get();
        let first_price_observation = self.get_price_observation(
            safe_price_current_index,
            safe_price_max_observations,
            &price_observations,
            start_round,
        );
        let last_price_observation = self.get_price_observation(
            safe_price_current_index,
            safe_price_max_observations,
            &price_observations,
            end_round,
        );

        self.compute_weighted_price(
            input_payment,
            &first_price_observation,
            &last_price_observation,
        )
    }

    #[label("safe-price-view")]
    #[view(getPriceObservation)]
    fn get_price_observation_view(&self, search_round: Round) -> PriceObservation<Self::Api> {
        let safe_price_current_index = self.safe_price_current_index().get();
        let safe_price_max_observations = self.safe_price_max_observations().get();
        let price_observations = self.price_observations().get();

        self.get_price_observation(
            safe_price_current_index,
            safe_price_max_observations,
            &price_observations,
            search_round,
        )
    }

    fn compute_weighted_price(
        &self,
        input_payment: EsdtTokenPayment<Self::Api>,
        first_price_observation: &PriceObservation<Self::Api>,
        last_price_observation: &PriceObservation<Self::Api>,
    ) -> EsdtTokenPayment<Self::Api> {
        let first_token_id = self.first_token_id().get();
        let second_token_id = self.second_token_id().get();

        require!(
            input_payment.token_identifier == first_token_id
                || input_payment.token_identifier == second_token_id,
            ERROR_BAD_INPUT_TOKEN
        );

        let (weighted_first_token_reserve, weighted_second_token_reserve) =
            self.compute_weighted_reserves(first_price_observation, last_price_observation);

        let output_payment = if input_payment.token_identifier == first_token_id {
            let output_amount =
                input_payment.amount * weighted_second_token_reserve / weighted_first_token_reserve;
            EsdtTokenPayment::new(second_token_id, 0, output_amount)
        } else {
            let output_amount =
                input_payment.amount * weighted_first_token_reserve / weighted_second_token_reserve;
            EsdtTokenPayment::new(first_token_id, 0, output_amount)
        };
        require!(output_payment.amount > 0u64, ERROR_ZERO_AMOUNT);
        output_payment
    }

    fn get_price_observation(
        &self,
        current_index: usize,
        max_observations: usize,
        price_observations: &ManagedVec<PriceObservation<Self::Api>>,
        search_round: Round,
    ) -> PriceObservation<Self::Api> {
        require!(
            !price_observations.is_empty(),
            ERROR_SAFE_PRICE_OBSERVATION_DOES_NOT_EXIST
        );

        // Check if the requested price observation is the last one
        let last_observation = price_observations.get(current_index);
        if last_observation.recording_round <= search_round {
            return last_observation;
        }

        // Check if the observation round exists in the list
        let oldest_observation_index = if price_observations.len() < max_observations {
            0
        } else {
            (current_index + 1) % max_observations
        };
        let oldest_observation_option = price_observations.try_get(oldest_observation_index);
        match oldest_observation_option {
            Some(oldest_observation) => {
                if search_round == oldest_observation.recording_round {
                    return oldest_observation;
                }
                require!(
                    search_round > oldest_observation.recording_round,
                    ERROR_SAFE_PRICE_OBSERVATION_DOES_NOT_EXIST
                );
            }
            None => sc_panic!(ERROR_SAFE_PRICE_OBSERVATION_DOES_NOT_EXIST),
        }

        let (mut price_observation, last_search_index) = self.price_observation_by_binary_search(
            current_index,
            max_observations,
            price_observations,
            search_round,
        );

        if price_observation.recording_round > 0 {
            return price_observation;
        }

        price_observation = self.price_observation_by_linear_interpolation(
            max_observations,
            price_observations,
            search_round,
            last_search_index,
        );

        price_observation
    }

    fn price_observation_by_binary_search(
        &self,
        current_index: usize,
        max_observations: usize,
        price_observations: &ManagedVec<PriceObservation<Self::Api>>,
        search_round: Round,
    ) -> (PriceObservation<Self::Api>, usize) {
        let mut search_index = 0;
        let mut left_index;
        let mut right_index;
        let observation_at_index_0 = price_observations.get(0);
        if observation_at_index_0.recording_round <= search_round {
            left_index = 0;
            right_index = current_index - 1;
        } else {
            left_index = current_index + 1;
            right_index = max_observations - 1;
        }

        while left_index <= right_index {
            search_index = (left_index + right_index) / 2;
            let price_observation = price_observations.get(search_index);
            match price_observation.recording_round.cmp(&search_round) {
                Ordering::Equal => return (price_observation, search_index),
                Ordering::Less => left_index = search_index + 1,
                Ordering::Greater => right_index = search_index - 1,
            }
        }

        (PriceObservation::default(), search_index)
    }

    fn price_observation_by_linear_interpolation(
        &self,
        max_observations: usize,
        price_observations: &ManagedVec<PriceObservation<Self::Api>>,
        search_round: Round,
        search_index: usize,
    ) -> PriceObservation<Self::Api> {
        let last_found_observation = price_observations.get(search_index);
        let left_observation;
        let right_observation;
        if last_found_observation.recording_round < search_round {
            left_observation = last_found_observation;
            let right_observation_index = (search_index + 1) % max_observations;
            right_observation = price_observations.get(right_observation_index);
        } else {
            let left_observation_index = if search_index == 0 {
                max_observations - 1
            } else {
                search_index - 1
            };
            left_observation = price_observations.get(left_observation_index);
            right_observation = last_found_observation;
        };

        let left_weight = search_round - left_observation.recording_round;
        let right_weight = right_observation.recording_round - search_round;
        let weight_sum = left_weight + right_weight;
        let first_token_reserve_sum = BigUint::from(left_weight)
            * left_observation.first_token_reserve_accumulated
            + BigUint::from(right_weight) * right_observation.first_token_reserve_accumulated;
        let second_token_reserve_sum = BigUint::from(left_weight)
            * left_observation.second_token_reserve_accumulated
            + BigUint::from(right_weight) * right_observation.second_token_reserve_accumulated;

        let first_token_reserve_accumulated = first_token_reserve_sum / weight_sum;
        let second_token_reserve_accumulated = second_token_reserve_sum / weight_sum;
        let weight_accumulated =
            left_observation.weight_accumulated + search_round - left_observation.recording_round;

        PriceObservation {
            first_token_reserve_accumulated,
            second_token_reserve_accumulated,
            weight_accumulated,
            recording_round: search_round,
        }
    }

    fn compute_weighted_reserves(
        &self,
        first_price_observation: &PriceObservation<Self::Api>,
        last_price_observation: &PriceObservation<Self::Api>,
    ) -> (BigUint, BigUint) {
        let weight_diff =
            last_price_observation.weight_accumulated - first_price_observation.weight_accumulated;
        if weight_diff == 0 {
            return (
                last_price_observation
                    .first_token_reserve_accumulated
                    .clone(),
                last_price_observation
                    .second_token_reserve_accumulated
                    .clone(),
            );
        }

        let first_token_reserve_diff = last_price_observation
            .first_token_reserve_accumulated
            .clone()
            - first_price_observation
                .first_token_reserve_accumulated
                .clone();
        let second_token_reserve_diff = last_price_observation
            .second_token_reserve_accumulated
            .clone()
            - first_price_observation
                .second_token_reserve_accumulated
                .clone();

        let weighted_first_token_reserve = first_token_reserve_diff / weight_diff;
        let weighted_second_token_reserve = second_token_reserve_diff / weight_diff;
        (weighted_first_token_reserve, weighted_second_token_reserve)
    }
}