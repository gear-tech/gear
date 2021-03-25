#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://substrate.dev/docs/en/knowledgebase/runtime/frame>

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod data;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::*;
	use sp_core::H256;
	use crate::data::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/// Programs runtime storage
	#[pallet::storage]
	#[pallet::getter(fn program)]
	pub type Programs<T> = StorageMap<
		_,
		Identity,
		H256,
		Program,
	>;

	/// Allocationns runtime storage
	#[pallet::storage]
	pub type Allocations<T> = StorageMap<
		_,
		Identity,
		u32,
		H256,
	>;

	/// Message queue runtime storage
	#[pallet::storage]
	pub type MessageQueue<T> = StorageValue<
		_,
		Vec<Message>,
	>;

	pub fn queue_message<T: Config>(message: Message) {
		let mut messages = <Pallet<T> as Store>::MessageQueue::get().unwrap_or_default();
		messages.push(message);
		<Pallet<T> as Store>::MessageQueue::set(Some(messages));
	}

	pub fn dequeue_message<T: Config>() -> Option<Message> {
		match <Pallet<T> as Store>::MessageQueue::get() {
			Some(mut messages) => {
				if messages.len() > 0 {
					let dequeued = messages.remove(0);
					<Pallet<T> as Store>::MessageQueue::set(Some(messages));
					Some(dequeued)
				} else {
					None
				}
			},
			None => None
		}
	}

	pub fn get_program<T: Config>(program_id: H256) -> Option<Program> {
		<Pallet<T> as Store>::Programs::get(program_id)
	}

	pub fn set_program<T: Config>(id: H256, program: Program) {
		<Pallet<T> as Store>::Programs::insert(id, program)
	}

	pub fn remove_program<T: Config>(id: H256) {
		<Pallet<T> as Store>::Programs::remove(id)
	}

	pub fn page_info<T: Config>(page: u32) -> Option<H256> {
		<Pallet<T> as Store>::Allocations::get(page)
	}

	pub fn alloc<T: Config>(page: u32, program: H256) {
		<Pallet<T> as Store>::Allocations::insert(page, program)
	}

	pub fn dealloc<T: Config>(page: u32) {
		<Pallet<T> as Store>::Allocations::remove(page)
	}

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Log event from the specific program
		Log(H256, Vec<u8>),
	}

	// Gear pallet error.
	#[pallet::error]
	pub enum Error<T> {
		/// Custom error.
		Custom,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T:Config> Pallet<T> {
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn submit_program(origin: OriginFor<T>, program: Program) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// Update storage.
			// <Programs<T>>::insert(H256::ranndom(), program);

			Ok(().into())
		}
	}
}
