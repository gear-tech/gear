use frame_support::{
    pallet_prelude::Weight,
    traits::{Currency, OnRuntimeUpgrade, ReservableCurrency},
};
use frame_system::AccountInfo;
use gear_common::{storage::LinkedNode, GasPrice, GasProvider, GasTree, Origin};
use gear_core::ids::ProgramId;
use sp_runtime::traits::{Get, UniqueSaturatedInto, Zero};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
type Balances<T> = pallet_balances::Pallet<T>;
type GearGas<T> = pallet_gear_gas::Pallet<T>;
type GearBank<T> = pallet_gear_bank::Pallet<T>;
type GasHandlerOf<T> = <GearGas<T> as GasProvider>::GasTree;
type GasNodesOf<T> = pallet_gear_gas::GasNodes<T>;
type AccountsOf<T> = frame_system::Account<T>;
type CurrencyOf<T> = <T as pallet_gear_bank::Config>::Currency;
type BalanceOf<T> = <CurrencyOf<T> as Currency<AccountIdOf<T>>>::Balance;
type DispatchesOf<T> = pallet_gear_messenger::Dispatches<T>;
type MailboxOf<T> = pallet_gear_messenger::Mailbox<T>;
type WaitlistOf<T> = pallet_gear_messenger::Waitlist<T>;
type DispatchStashOf<T> = pallet_gear_messenger::DispatchStash<T>;

pub struct MigrateToGearBank<T, P>(sp_std::marker::PhantomData<(T, P)>)
where
    T: frame_system::Config<AccountData = pallet_balances::AccountData<BalanceOf<T>>>
        + pallet_balances::Config<Balance = BalanceOf<T>>
        + pallet_gear_gas::Config
        + pallet_gear_bank::Config
        + pallet_gear_messenger::Config,
    P: GasPrice<Balance = BalanceOf<T>>,
    AccountIdOf<T>: Origin;

impl<T, P> OnRuntimeUpgrade for MigrateToGearBank<T, P>
where
    T: frame_system::Config<AccountData = pallet_balances::AccountData<BalanceOf<T>>>
        + pallet_balances::Config<Balance = BalanceOf<T>>
        + pallet_gear_gas::Config
        + pallet_gear_bank::Config
        + pallet_gear_messenger::Config,
    P: GasPrice<Balance = BalanceOf<T>>,
    AccountIdOf<T>: Origin,
{
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        Ok(Default::default())
    }

    fn on_runtime_upgrade() -> Weight {
        let version = T::Version::get().spec_version;

        log::info!("🚚 Running migration to gear-bank with current spec version {version:?}");

        // TODO: consider version here.
        if version <= 220 {
            let mut ops = 0u64;

            // Depositing gas from gas nodes.
            let gas_nodes_iter = GasNodesOf::<T>::iter();
            for (node_id, gas_node) in gas_nodes_iter {
                let external = GasHandlerOf::<T>::get_external(node_id)
                    .expect("Failed to get external id of the node");
                let gas_amount = gas_node.total_value();

                let gas_price = P::gas_price(gas_amount);
                Balances::<T>::unreserve(&external, gas_price);
                GearBank::<T>::deposit_gas::<P>(&external, gas_amount)
                    .expect("Failed to deposit gas");

                // Just random approximate amount of operations,
                // that will be meant as write operations.
                ops += 4;
            }

            let mut deposit = |source: ProgramId, value: u128| {
                let source = AccountIdOf::<T>::from_origin(source.into_origin());
                let value = value.unique_saturated_into();
                Balances::<T>::unreserve(&source, value);
                GearBank::<T>::deposit_value(&source, value).expect("Failed to deposit value");

                // Just random approximate amount of operations,
                // that will be meant as write operations.
                ops += 3;
            };

            // Dispatches value migration.
            let dispatches_iter = DispatchesOf::<T>::iter_values();
            for LinkedNode {
                value: dispatch, ..
            } in dispatches_iter
            {
                deposit(dispatch.source(), dispatch.value());
            }

            // Mailbox value migration.
            let mailbox_iter = MailboxOf::<T>::iter_values();
            for (message, _) in mailbox_iter {
                deposit(message.source(), message.value());
            }

            // Waitlist value migration.
            let waitlist_iter = WaitlistOf::<T>::iter_values();
            for (dispatch, _) in waitlist_iter {
                deposit(dispatch.source(), dispatch.value());
            }

            // DispatchStash value migration.
            let dispatch_stash_iter = DispatchStashOf::<T>::iter_values();
            for (dispatch, _) in dispatch_stash_iter {
                deposit(dispatch.source(), dispatch.value());
            }

            // Depositing value.
            let accounts_iter = AccountsOf::<T>::iter();
            for (account_id, AccountInfo { data, .. }) in accounts_iter {
                let reserve = data.reserved;
                if !reserve.is_zero() {
                    Balances::<T>::unreserve(&account_id, reserve);
                }
            }

            T::DbWeight::get().writes(ops)
        } else {
            log::info!(
                "❌ Migration to gear-bank did not execute. This probably should be removed"
            );
            Zero::zero()
        }
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: Vec<u8>) -> Result<(), &'static str> {
        log::info!("Runtime successfully migrated to gear-bank.");
        Ok(())
    }
}
