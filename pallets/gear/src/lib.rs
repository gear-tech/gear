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
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::*;
	use sp_core::H256;
	use sp_std::prelude::*;
	use common::{self, Message, Program};

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}

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
	}

	// Gear pallet error.
	#[pallet::error]
	pub enum Error<T> {
		/// Custom error.
		Custom,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Initialization
		fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
			0
		}

		/// Finalization
		fn on_finalize(_bn: BlockNumberFor<T>) {
			// At the end of the block, we process all queued messages
			// TODO: When gas is introduced, processing should be limited to the specific max gas
			// TODO: When memory regions introduced, processing should be limited to the messages that touch 
			//       specific pages.
			loop {
				match rti::gear_executor::process() {
					Ok(execution_report) => {
						if execution_report.handled == 0 { break; }

						for (program_id, payload) in execution_report.log.into_iter() {
							Self::deposit_event(Event::Log(program_id, payload));
						}
					},
					Err(_e) => {
						// TODO: make error event log record
						continue;
					},
				}
			}
		}
	}

	#[pallet::call]
	impl<T:Config> Pallet<T> {
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn submit_program(origin: OriginFor<T>, program: Program) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			let nonce = frame_system::Account::<T>::get(who.clone()).nonce;

			let mut data = Vec::new();
			program.encode_to(&mut data);
			who.encode_to(&mut data);
			nonce.encode_to(&mut data);

			let id: H256 = sp_io::hashing::blake2_256(&data[..]).into();

			common::set_program(id.clone(), program);

			Self::deposit_event(Event::NewProgram(id));

			Ok(().into())
		}

		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn send_message(origin: OriginFor<T>, destination: H256, payload: Vec<u8>) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			common::queue_message(Message{
				source: H256::default(),
				dest: destination,
				payload: payload,
			});

			Ok(().into())
		}
	}
}
