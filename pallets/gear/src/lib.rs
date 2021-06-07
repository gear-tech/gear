// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://substrate.dev/docs/en/knowledgebase/runtime/frame>

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		dispatch::DispatchResultWithPostInfo,
		pallet_prelude::*,
		traits::{Currency, ExistenceRequirement, ReservableCurrency, BalanceStatus},
		weights::{IdentityFee, WeightToFeePolynomial},
	};
	use frame_system::pallet_prelude::*;
	use sp_core::H256;
	use sp_std::prelude::*;
	use common::{self, Message, Origin, IntermediateMessage, MessageOrigin, MessageRoute};
	use sp_inherents::{InherentIdentifier, ProvideInherent, InherentData};

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Gas and value transfer currency
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

		#[pallet::constant]
		type SubmitWeightPerByte: Get<u64>;

		#[pallet::constant]
		type MessagePerByte: Get<u64>;
	}

	type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Log event from the specific program.
		Log(H256, Vec<u8>),
		/// Program created in the network.
		NewProgram(H256),
		/// Program initialization error.
		InitFailure(H256, MessageError),
		/// Program initialized.
		ProgramInitialized(H256),
		/// Some number of messages processed.
		MessagesDequeued(u32),
		/// Message dispatch resulted in error
		MessageNotProcessed(MessageError),
	}

	// Gear pallet error.
	#[pallet::error]
	pub enum Error<T> {
		/// Not enough balance to reserve.
		///
		/// Usually occurs when gas_limit specified is such that origin account can't afford the message.
		NotEnoughBalanceForReserve,
	}

	#[derive(Debug, Encode, Decode, Clone, PartialEq)]
	pub enum MessageError {
		ValueTransfer,
		Dispatch,
	}

	#[pallet::storage]
	pub type MessageQueue<T> = StorageValue<_, Vec<IntermediateMessage>>;

	#[pallet::storage]
	pub type DequeueLimit<T> = StorageValue<_, u32>;

	#[pallet::storage]
	pub type MessagesProcessed<T> = StorageValue<_, u32>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Initialization
		fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
			0
		}

		/// Finalization
		fn on_finalize(_bn: BlockNumberFor<T>) {
		}
	}

	fn gas_to_fee<T: Config>(gas: u64) -> BalanceOf<T>
	where
		<T::Currency as Currency<T::AccountId>>::Balance : Into<u128> + From<u128>,
	{
		IdentityFee::<BalanceOf<T>>::calc(&gas)
	}

	fn block_author<T: Config + pallet_authorship::Config>() -> Option<T::AccountId> {
		Some(<pallet_authorship::Module<T>>::author())
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Origin,
		T: pallet_authorship::Config,
		<T::Currency as Currency<T::AccountId>>::Balance : Into<u128> + From<u128>,
	{
		#[pallet::weight(
			T::DbWeight::get().writes(4) +
			T::SubmitWeightPerByte::get()*(code.len() as u64) +
			T::MessagePerByte::get()*(init_payload.len() as u64)
		)]
		pub fn submit_program(
			origin: OriginFor<T>,
			code: Vec<u8>,
			salt: Vec<u8>,
			init_payload: Vec<u8>,
			gas_limit: u64,
			value: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			let reserve_fee = gas_to_fee::<T>(gas_limit);

			// First we reserve enough funds on the account to pay for 'gas_limit'
			// and to transfer declared value.
			T::Currency::reserve(&who, reserve_fee + value)
				.map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

			let mut data = Vec::new();
			code.encode_to(&mut data);
			salt.encode_to(&mut data);

			let id: H256 = sp_io::hashing::blake2_256(&data[..]).into();

			<MessageQueue<T>>::mutate(|messages| {
				let mut actual_messages = messages.take().unwrap_or_default();
				actual_messages.push(IntermediateMessage::InitProgram {
					external_origin: who.into_origin(),
					code,
					program_id: id,
					payload: init_payload,
					gas_limit,
					value: value.into(),
				});

				*messages = Some(actual_messages);
			});

			Self::deposit_event(Event::NewProgram(id));

			Ok(().into())
		}

		#[pallet::weight(
			T::DbWeight::get().writes(4) +
			*gas_limit +
			T::MessagePerByte::get()*(payload.len() as u64)
		)]
		pub fn send_message(
			origin: OriginFor<T>,
			destination: H256,
			payload: Vec<u8>,
			gas_limit: u64,
			value: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			let gas_limit_reserve = gas_to_fee::<T>(gas_limit);

			// First we reserve enough funds on the account to pay for 'gas_limit'
			T::Currency::reserve(&who, gas_limit_reserve)
				.map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

			// Since messages a guaranteed to be dispatched, we transfer value immediately
			T::Currency::transfer(
				&who,
				&<T::AccountId as Origin>::from_origin(destination),
				value,
				ExistenceRequirement::AllowDeath,
			)?;

			// Only after reservation the message is actually put in the queue.
			<MessageQueue<T>>::mutate(|messages| {
				let mut actual_messages = messages.take().unwrap_or_default();

				let nonce = common::nonce_fetch_inc();

				let mut message_id = payload.encode();
				message_id.extend_from_slice(&nonce.to_le_bytes());
				let message_id: H256 = sp_io::hashing::blake2_256(&message_id).into();

				actual_messages.push(IntermediateMessage::DispatchMessage {
					id: message_id,
					route: MessageRoute {
						origin: MessageOrigin::External(who.into_origin()),
						destination,
					},
					payload,
					gas_limit,
					value: value.into()
				});

				*messages = Some(actual_messages);
			});

			Ok(().into())
		}

		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn process_queue(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			// At the beginning of a new block, we process all queued messages
			// TODO: When gas is introduced, processing should be limited to the specific max gas
			// TODO: When memory regions introduced, processing should be limited to the messages that touch
			//       specific pages.

			let messages = <MessageQueue<T>>::take().unwrap_or_default();
			let messages_processed = <MessagesProcessed<T>>::get().unwrap_or(0);

			if <DequeueLimit<T>>::get().map(|limit| limit <= messages_processed).unwrap_or(false) {
				return Ok(().into());
			}

			let mut stop_list = Vec::new();
			let mut total_handled = 0u32;

			for message in messages {
				match message {
					// Initialization queue is handled separately and on the first place
					// Any programs failed to initialize are deleted and further messages to them are not processed
					//
					// TODO: also process `external_origin` once origins are introduced
					IntermediateMessage::InitProgram {
						external_origin, code, program_id, payload, gas_limit, value
					} => {
						match rti::gear_executor::init_program(program_id, code, payload, gas_limit, value) {
							Err(_) => {
								stop_list.push(program_id);
								Self::deposit_event(Event::InitFailure(program_id, MessageError::Dispatch));
							},
							Ok(execution_report) => {

								// In case of init, we can unreserve everything right away.
								T::Currency::unreserve(
									&<T::AccountId as Origin>::from_origin(external_origin),
									gas_to_fee::<T>(gas_limit) + value.into(),
								);

								if let Err(_) = T::Currency::transfer(
									&<T::AccountId as Origin>::from_origin(external_origin),
									&<T::AccountId as Origin>::from_origin(program_id),
									value.into(),
									ExistenceRequirement::AllowDeath,
								) {
									// if transfer failed, gas spent and gas left does not matter since initialization
									// failed, and we unreserved gas_limit deposit already above.
									Self::deposit_event(Event::InitFailure(program_id, MessageError::ValueTransfer));
								} else {
									Self::deposit_event(Event::ProgramInitialized(program_id));
									total_handled += 1;

									// handle refunds
									for (destination, gas_charge) in execution_report.gas_charges {
										// TODO: weight to fee calculator might not be identity fee
										let charge = gas_to_fee::<T>(gas_charge);

										if let Err(_) = T::Currency::transfer(
											&<T::AccountId as Origin>::from_origin(destination),
											// block author destination or _burn_
											// TODO: audit if this is correct
											&block_author::<T>().unwrap_or(<T::AccountId as Origin>::from_origin(H256::zero())),
											charge,
											ExistenceRequirement::AllowDeath,
										) {
											// should not be possible since there should've been reserved enough for
											// the transfer
											// TODO: audit this
										}
									}

									for (program_id, payload) in execution_report.log {
										Self::deposit_event(Event::Log(program_id, payload));
									}
								}
							}
						}
					},
					IntermediateMessage::DispatchMessage {
						id, route, payload, gas_limit, value
					} => {
						let source = match route.origin {
							MessageOrigin::External(_) => H256::zero(),
							MessageOrigin::Internal(program_id) => program_id,
						};

						common::queue_message(Message{
							source,
							payload,
							gas_limit: Some(gas_limit),
							dest: route.destination,
							value,
						}, id);
					}
				}
			}

			loop {
				match rti::gear_executor::process() {
					Ok(execution_report) => {
						total_handled += execution_report.handled;

						<MessagesProcessed<T>>::mutate(|messages_processed| *messages_processed = Some(messages_processed.unwrap_or(0) + execution_report.handled));
						let messages_processed = <MessagesProcessed<T>>::get().unwrap_or(0);
						if <DequeueLimit<T>>::get().is_some() {
							if <DequeueLimit<T>>::get().map(|limit| limit <= messages_processed).unwrap_or(false) {
								break;
							}
						}
						if execution_report.handled == 0 { break; }

						for (destination, gas_left) in execution_report.gas_refunds {
							let refund = gas_to_fee::<T>(gas_left);

							let _ = T::Currency::unreserve(
								&<T::AccountId as Origin>::from_origin(destination),
								refund,
							);
						}

						for (destination, gas_charge) in execution_report.gas_charges {
							let charge = gas_to_fee::<T>(gas_charge);

							let _ = T::Currency::repatriate_reserved(
								&<T::AccountId as Origin>::from_origin(destination),
								&block_author::<T>().unwrap_or(<T::AccountId as Origin>::from_origin(H256::zero())),
								charge,
								BalanceStatus::Free,
							);
						}

						for (program_id, payload) in execution_report.log {
							Self::deposit_event(Event::Log(program_id, payload));
						}
					},
					Err(_e) => {
						// TODO: make error event log record
						continue;
					},
				}
			}

			Self::deposit_event(Event::MessagesDequeued(total_handled));

			Ok(().into())
		}
	}

	impl<T: Config> ProvideInherent for Pallet<T>
	where
		T::AccountId: Origin,
		T: pallet_authorship::Config,
		<T::Currency as Currency<T::AccountId>>::Balance : Into<u128> + From<u128>,
	{
		type Call = Call<T>;
		type Error = sp_inherents::MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier = *b"gprocess";

		fn create_inherent(_data: &InherentData) -> Option<Self::Call> {
			Some(Call::process_queue())
		}
	}
}
