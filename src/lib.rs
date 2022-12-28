#![no_std]

mod rewards;
mod snapshots;
mod tokens;

use elrond_wasm::types::heap::Vec;
use rewards::TokenAndBalance;

elrond_wasm::imports!();
elrond_wasm::derive_imports!();

static ERR_NOT_IN_SNAPSHOT_PERIOD: &[u8] = b"Not in snapshot period";
static ERR_NOT_IN_DISTRIUTION_PERIOD: &[u8] = b"Not in distribution period";
static ERR_DISTRIBUTION_NOT_COMPLETE: &[u8] = b"Distribution is not complete";

#[derive(TopEncode, TypeAbi)]
pub struct StakingState {
    current_round: u32,
    is_accumulation_period: bool,
    is_distribution_period: bool,
}

#[derive(PartialEq, TopDecode, TopEncode, TypeAbi)]
pub enum RoundState {
    HoldersSnapshot,
    RewardsDistribution,
    Complete,
}

#[elrond_wasm::derive::contract]
pub trait ScStaking:
    rewards::RewardsModule + snapshots::SnapshotsModule + tokens::TokensModule
{
    // init

    #[init]
    fn init(&self) {
        self.current_round().set_if_empty(1);
        self.current_state()
            .set_if_empty(RoundState::HoldersSnapshot);
    }

    // owner endpoints

    #[only_owner]
    #[endpoint]
    fn configure(
        &self,
        treasury_address: ManagedAddress,
        team_a_address: ManagedAddress,
        team_j_address: ManagedAddress,
        team_p_address: ManagedAddress,
    ) {
        self.treasury_address().set(treasury_address);
        self.team_a_address().set(team_a_address);
        self.team_j_address().set(team_j_address);
        self.team_p_address().set(team_p_address);
    }

    #[only_owner]
    #[endpoint(configureToken)]
    fn configure_token(&self, token_identifier: TokenIdentifier, nonce: u64, threshold: BigUint) {
        self.configure_token_inner(&token_identifier, nonce, &threshold);
    }

    #[only_owner]
    #[endpoint(initRound)]
    fn init_round(&self) {
        self.require_distribution_complete();

        self.current_round().update(|x| *x += 1);

        self.reset_snapshots();
        self.current_state().set(RoundState::HoldersSnapshot);
    }

    #[only_owner]
    #[endpoint]
    fn snapshot(
        &self,
        addresses_and_balances: MultiValueEncoded<MultiValue2<ManagedAddress, BigUint>>,
    ) {
        self.require_snapshot_period();
        self.snapshot_internal(self.current_round().get(), addresses_and_balances);
    }

    #[payable("*")]
    #[endpoint(fundRewards)]
    fn fund_rewards(&self) {
        self.require_snapshot_period();

        self.fund_rewards_internal();
    }

    #[only_owner]
    #[endpoint(prepareRewards)]
    fn prepare_rewards(&self) {
        self.require_snapshot_period();

        let current_round = self.current_round().get();

        self.prepare_rewards_internal(current_round);

        self.current_state().set(RoundState::RewardsDistribution);
    }

    #[only_owner]
    #[endpoint]
    fn distribute(&self, limit: usize) {
        require!(
            self.current_state().get() == RoundState::RewardsDistribution,
            ERR_NOT_IN_DISTRIUTION_PERIOD
        );

        let current_round = self.current_round().get();

        let distribution_complete = self.distribute_rewards_internal(current_round, limit);
        if distribution_complete {
            self.current_state().set(RoundState::Complete);
        }
    }

    // functions

    fn require_distribution_complete(&self) {
        require!(
            self.current_state().get() == RoundState::Complete,
            ERR_DISTRIBUTION_NOT_COMPLETE
        )
    }

    fn require_snapshot_period(&self) {
        require!(
            self.current_state().get() == RoundState::HoldersSnapshot,
            ERR_NOT_IN_SNAPSHOT_PERIOD
        );
    }

    // storage & views

    #[view(getState)]
    fn state(&self) -> StakingState {
        let round = self.current_round().get();
        let accumulation = self.rewards_for_round(round).is_empty();
        let distribution = !accumulation && !self.all_addresses().is_empty();
        let state = StakingState {
            current_round: round,
            is_accumulation_period: accumulation,
            is_distribution_period: distribution,
        };
        return state;
    }

    #[view(getCurrentRound)]
    #[storage_mapper("current_round")]
    fn current_round(&self) -> SingleValueMapper<u32>;

    #[view(getCurrentState)]
    #[storage_mapper("current_state")]
    fn current_state(&self) -> SingleValueMapper<RoundState>;

    #[view(getCurrentRoundRewards)]
    fn get_current_round_rewards(&self) -> MultiValueEncoded<rewards::TokenAndBalance<Self::Api>> {
        let current_round = self.current_round().get();

        let rewards_: Vec<rewards::TokenAndBalance<Self::Api>>;
        if self.rewards_for_round(current_round).is_empty() {
            let calculated_rewards = &mut Vec::<rewards::TokenAndBalance<Self::Api>>::new();
            self.calculate_current_rewards(calculated_rewards);
            rewards_ = calculated_rewards.to_vec();
        } else {
            rewards_ = self.rewards_for_round(current_round).iter().collect();
        }

        let result: ManagedVec<Self::Api, TokenAndBalance<Self::Api>> = rewards_.into();
        return result.into();
    }
}
