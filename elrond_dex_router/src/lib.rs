#![no_std]

elrond_wasm::imports!();
elrond_wasm::derive_imports!();

pub mod factory;
pub use factory::*;

#[elrond_wasm_derive::callable(PairContractProxy)]
pub trait PairContract {
	fn set_fee_on_endpoint(
		&self, 
		enabled: bool, 
		fee_to_address: Address, 
		fee_token: TokenIdentifier
	) -> ContractCall<BigUint, ()>;
	fn set_lp_token_identifier_endpoint(&self, token_identifier: TokenIdentifier) -> ContractCall<BigUint, ()>;
	fn get_lp_token_identifier_endpoint(&self) -> ContractCall<BigUint, TokenIdentifier>;
}

#[elrond_wasm_derive::contract(RouterImpl)]
pub trait Router {

	#[module(FactoryModuleImpl)]
	fn factory(&self) -> FactoryModuleImpl<T, BigInt, BigUint>;

	#[init]
	fn init(&self) {
		self.factory().init();
	}

	//ENDPOINTS
	#[endpoint(createPair)]
	fn create_pair(&self, token_a: TokenIdentifier, token_b: TokenIdentifier) -> SCResult<Address> {
		require!(token_a != token_b, "Identical tokens");
		require!(token_a.is_esdt(), "Only esdt tokens allowed");
		require!(token_b.is_esdt(), "Only esdt tokens allowed");
		let pair_address = self.get_pair(token_a.clone(), token_b.clone());
		require!(pair_address == Address::zero(), "Pair already existent");
		Ok(self.factory().create_pair(&token_a, &token_b))
	}

	#[payable("EGLD")]
	#[endpoint(issueLpToken)]
	fn issue_lp_token_endpoint(
		&self,
		address: Address,
		tp_token_display_name: BoxedBytes,
		tp_token_ticker: BoxedBytes,
		#[payment] issue_cost: BigUint
	) -> SCResult<AsyncCall<BigUint>> {
		let half_gas = self.get_gas_left() / 2;
		let result = contract_call!(self, address.clone(), PairContractProxy)
            .get_lp_token_identifier_endpoint()
            .execute_on_dest_context(half_gas, self.send());

		require!(result.is_egld(), "PAIR: LP Token already issued.");

		Ok(ESDTSystemSmartContractProxy::new()
			.issue_fungible(
				issue_cost,
				&tp_token_display_name,
				&tp_token_ticker,
				&BigUint::from(1000u64),
				FungibleTokenProperties {
					num_decimals: 18,
					can_freeze: true,
					can_wipe: true,
					can_pause: true,
					can_mint: true,
					can_burn: true,
					can_change_owner: true,
					can_upgrade: true,
					can_add_special_roles: true,
				},
			)
			.async_call()
			.with_callback(self.callbacks().lp_token_issue_callback(address)))
	}

	#[endpoint(setLocalRoles)]
	fn set_local_roles(
		&self,
		address: Address,
		token_identifier: TokenIdentifier,
		#[var_args] roles: VarArgs<EsdtLocalRole>,
	) -> AsyncCall<BigUint> {
		ESDTSystemSmartContractProxy::new()
			.set_special_roles(
				&address,
				token_identifier.as_esdt_identifier(),
				roles.as_slice(),
			)
			.async_call()
			.with_callback(self.callbacks().change_roles_callback())
	}

	#[endpoint(setStakingInfo)]
	fn set_staking_info(
		&self, 
		staking_address: Address, 
		staking_token: TokenIdentifier
	) -> SCResult<()> {
		only_owner!(self, "Permission denied");
		self.staking_address().set(&staking_address);
		self.staking_token().set(&staking_token);
		Ok(())
	}

	fn check_is_pair_sc(&self, pair_address: &Address) -> SCResult<()> {
		require!(
			self.factory()
				.pair_map()
				.values()
				.any(|address| &address == pair_address),
			"Not a pair SC"
		);
		Ok(())
	}

	#[endpoint(upgradePair)]
	fn upgrade_pair(&self, pair_address: Address) -> SCResult<()> {
		only_owner!(self, "Permission denied");
		sc_try!(self.check_is_pair_sc(&pair_address));

		self.factory().upgrade_pair(&pair_address);
		Ok(())
	}

	#[endpoint(setFeeOn)]
	fn set_fee_on(&self, pair_address: Address) -> SCResult<AsyncCall<BigUint>> {
		only_owner!(self, "Permission denied");
		sc_try!(self.check_is_pair_sc(&pair_address));

		let staking_token = self.staking_token().get();
		let staking_address = self.staking_address().get();
		Ok(contract_call!(self, pair_address, PairContractProxy)
			.set_fee_on_endpoint(true, staking_address, staking_token)
			.async_call())
	}

	#[endpoint(setFeeOff)]
	fn set_fee_off(&self, pair_address: Address) -> SCResult<AsyncCall<BigUint>> {
		only_owner!(self, "Permission denied");
		sc_try!(self.check_is_pair_sc(&pair_address));

		Ok(contract_call!(self, pair_address, PairContractProxy)
			.set_fee_on_endpoint(false, Address::zero(), TokenIdentifier::egld())
			.async_call())
	}

	#[endpoint(startPairCodeConstruction)]
	fn start_pair_code_construction(&self) -> SCResult<()> {
		only_owner!(self, "Permission denied");

		self.factory().start_pair_construct();
		Ok(())
	}

	#[endpoint(endPairCodeConstruction)]
	fn end_pair_code_construction(&self) -> SCResult<()> {
		only_owner!(self, "Permission denied");

		self.factory().end_pair_construct();
		Ok(())
	}

	#[endpoint(appendPairCode)]
	fn apppend_pair_code(&self, part: BoxedBytes) -> SCResult<()> {		
		only_owner!(self, "Permission denied");

		self.factory().append_pair_code(&part);
		Ok(())
	}

	//VIEWS
	#[view(getPair)]
	fn get_pair(&self, token_a: TokenIdentifier, token_b: TokenIdentifier) -> Address {
		let mut address = self.factory().pair_map().get(&(token_a.clone(), token_b.clone())).unwrap_or(Address::zero());
		if address == Address::zero() {
			address = self.factory().pair_map().get(&(token_b.clone(), token_a.clone())).unwrap_or(Address::zero());
		}
		address
	}

	#[view(getAllPairs)]
	fn get_all_pairs(&self) -> MultiResultVec<Address> {
		self.factory().pair_map_values()
	}

	#[callback]
	fn lp_token_issue_callback(
		&self,
		address: Address,
		#[payment_token] token_identifier: TokenIdentifier,
		#[payment] returned_tokens: BigUint,
		#[call_result] result: AsyncCallResult<()>,
	) {
		let success;
		match result {
			AsyncCallResult::Ok(()) => {
				let half_gas = self.get_gas_left() / 2;
				
				let _ = contract_call!(self, address, PairContractProxy)
            					.set_lp_token_identifier_endpoint(token_identifier.clone())
            					.execute_on_dest_context(half_gas, self.send());
				success = true;
				
			},
			AsyncCallResult::Err(_) => {
				success = false;
			},
		}

		if success == false {
			if token_identifier.is_egld() && returned_tokens > 0 {
				self.send().direct_egld(&self.get_caller(), &returned_tokens, &[]);
			}
		}
	}

	#[callback]
	fn change_roles_callback(&self, #[call_result] result: AsyncCallResult<()>) {
		match result {
			AsyncCallResult::Ok(()) => {
				self.last_error_message().clear();
			},
			AsyncCallResult::Err(message) => {
				self.last_error_message().set(&message.err_msg);
			},
		}
	}

	#[view(getStakingAddress)]
	#[storage_mapper("staking_address")]
	fn staking_address(&self) -> SingleValueMapper<Self::Storage, Address>;

	#[view(getStakingToken)]
	#[storage_mapper("staking_token")]
	fn staking_token(&self) -> SingleValueMapper<Self::Storage, TokenIdentifier>;

	#[view(lastErrorMessage)]
	#[storage_mapper("lastErrorMessage")]
	fn last_error_message(&self) -> SingleValueMapper<Self::Storage, BoxedBytes>;
}