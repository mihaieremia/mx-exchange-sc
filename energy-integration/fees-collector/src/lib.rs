#![no_std]
#![feature(generic_associated_types)]

use common_types::{PaymentsVec, Week};
use energy_query::Energy;
use weekly_rewards_splitting::ongoing_operation::{
    CONTINUE_OP, DEFAULT_MIN_GAS_TO_SAVE_PROGRESS, STOP_OP,
};

elrond_wasm::imports!();

pub mod config;
pub mod fees_accumulation;

#[elrond_wasm::contract]
pub trait FeesCollector:
    config::ConfigModule
    + weekly_rewards_splitting::WeeklyRewardsSplittingModule
    + weekly_rewards_splitting::ongoing_operation::OngoingOperationModule
    + fees_accumulation::FeesAccumulationModule
    + energy_query::EnergyQueryModule
    + week_timekeeping::WeekTimekeepingModule
    + elrond_wasm_modules::pause::PauseModule
{
    #[init]
    fn init(&self) {
        let current_epoch = self.blockchain().get_block_epoch();
        self.first_week_start_epoch().set_if_empty(current_epoch);
    }

    #[endpoint(claimRewards)]
    fn claim_rewards(&self) -> PaymentsVec<Self::Api> {
        require!(self.not_paused(), "Cannot claim while paused");

        self.claim_multi(|week: Week| self.collect_accumulated_fees_for_week(week))
    }

    /// Accepts pairs of (user address, energy amount, total locked tokens).
    /// Sets the given amounts for the user's positions,
    /// and recomputes the global amounts.
    ///
    /// Returns whether the operation was fully completed.
    /// If not, it also returns the last processed index.
    #[only_owner]
    #[endpoint(recomputeEnergy)]
    fn recompute_energy(
        &self,
        arg_pairs: MultiValueEncoded<MultiValue3<ManagedAddress, BigUint, BigUint>>,
    ) -> MultiValue2<OperationCompletionStatus, OptionalValue<usize>> {
        require!(self.is_paused(), "May only recompute while paused");

        let current_week = self.get_current_week();
        let current_epoch = self.blockchain().get_block_epoch();

        let mut iter = arg_pairs.into_iter().enumerate();
        let mut last_processed_index = 0;

        let run_result = self.run_while_it_has_gas(DEFAULT_MIN_GAS_TO_SAVE_PROGRESS, || match iter
            .next()
        {
            Some((index, multi_value)) => {
                let (user, energy, total_locked) = multi_value.into_tuple();
                let energy_entry = Energy::new(BigInt::from(energy), current_epoch, total_locked);
                self.update_user_energy_for_current_week(&user, current_week, &energy_entry);

                self.current_claim_progress(&user).update(|claim_progress| {
                    if claim_progress.week == current_week {
                        claim_progress.energy = energy_entry;
                    }
                });

                last_processed_index = index;

                CONTINUE_OP
            }
            None => STOP_OP,
        });

        match run_result {
            OperationCompletionStatus::Completed => (run_result, OptionalValue::None).into(),
            OperationCompletionStatus::InterruptedBeforeOutOfGas => {
                (run_result, OptionalValue::Some(last_processed_index)).into()
            }
        }
    }
}