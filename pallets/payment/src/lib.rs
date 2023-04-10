// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
// (issue #2531)
#![allow(deprecated)]

use common::{storage::*, ExtractCall};
use frame_support::{
    dispatch::{DispatchInfo, GetDispatchInfo, PostDispatchInfo},
    pallet_prelude::*,
    traits::Contains,
};
use pallet_transaction_payment::{
    ChargeTransactionPayment, FeeDetails, Multiplier, MultiplierUpdate, OnChargeTransaction,
    RuntimeDispatchInfo,
};
use sp_runtime::{
    traits::{Bounded, Convert, DispatchInfoOf, Dispatchable, PostDispatchInfoOf, SignedExtension},
    transaction_validity::TransactionValidityError,
    FixedPointNumber, FixedPointOperand, Perquintill, SaturatedConversion,
};
use sp_std::borrow::Cow;

pub use pallet::*;

type BalanceOf<T> =
    <<T as pallet_transaction_payment::Config>::OnChargeTransaction as OnChargeTransaction<T>>::Balance;
type CallOf<T> = <T as frame_system::Config>::RuntimeCall;
pub(crate) type QueueOf<T> = <<T as Config>::Messenger as Messenger>::Queue;
pub type TransactionPayment<T> = pallet_transaction_payment::Pallet<T>;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

/// A wrapper around the `pallet_transaction_payment::ChargeTransactionPayment`.
/// Adjusts `DispatchInfo` to reflect custom fee add-ons.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct CustomChargeTransactionPayment<T: Config>(ChargeTransactionPayment<T>);

impl<T: Config> CustomChargeTransactionPayment<T>
where
    BalanceOf<T>: Send + Sync + FixedPointOperand,
    CallOf<T>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
    pub fn from(tip: BalanceOf<T>) -> Self {
        Self(ChargeTransactionPayment::<T>::from(tip))
    }
}

impl<T: Config> sp_std::fmt::Debug for CustomChargeTransactionPayment<T> {
    #[cfg(feature = "std")]
    fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
        write!(f, "CustomChargeTransactionPayment({:?})", self.0)
    }
    #[cfg(not(feature = "std"))]
    fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
        Ok(())
    }
}

// Follow pallet-transaction-payment implementation
impl<T: Config> SignedExtension for CustomChargeTransactionPayment<T>
where
    T: TypeInfo,
    BalanceOf<T>: Send + Sync + From<u64> + FixedPointOperand,
    CallOf<T>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
    const IDENTIFIER: &'static str = <ChargeTransactionPayment<T> as SignedExtension>::IDENTIFIER;
    type AccountId = <ChargeTransactionPayment<T> as SignedExtension>::AccountId;
    type Call = CallOf<T>;
    type AdditionalSigned = <ChargeTransactionPayment<T> as SignedExtension>::AdditionalSigned;
    type Pre = <ChargeTransactionPayment<T> as SignedExtension>::Pre;
    fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
        self.0.additional_signed()
    }

    fn validate(
        &self,
        who: &Self::AccountId,
        call: &Self::Call,
        info: &DispatchInfoOf<Self::Call>,
        len: usize,
    ) -> TransactionValidity {
        // Override DispatchInfo struct for call variants exempted from weight fee multiplication
        let info = Self::pre_dispatch_info(call, info);
        self.0.validate(who, call, &info, len)
    }

    fn pre_dispatch(
        self,
        who: &Self::AccountId,
        call: &Self::Call,
        info: &DispatchInfoOf<Self::Call>,
        len: usize,
    ) -> Result<Self::Pre, TransactionValidityError> {
        // Override DispatchInfo struct for call variants exempted from weight fee multiplication
        let info = Self::pre_dispatch_info(call, info);
        self.0.pre_dispatch(who, call, &info, len)
    }

    fn post_dispatch(
        maybe_pre: Option<Self::Pre>,
        info: &DispatchInfoOf<Self::Call>,
        post_info: &PostDispatchInfoOf<Self::Call>,
        len: usize,
        result: &sp_runtime::DispatchResult,
    ) -> Result<(), TransactionValidityError> {
        // There is no easy way to modify the original `DispatchInfo` struct similarly
        // it's done in `pre_dispatch()` because a call is not supplied.
        // However, we can just leave it as is and yet get the correct fee refund if any:
        //   - if `None` is returned as the actual weight (i.e. worst case) nothing is supposed
        //   to be refunded anyway and saturating subtraction guarantees we don't have overflow;
        //   - if `post_info` has `Some(actual_weight)`, the minimum of it and `info.weight` will
        //   be used to calculate the correct fee so it is just our responsibility to do
        //   weight normalization before returning it from the extrinsic.
        //
        // TODO: still think of a more robust way to deal with fee refunds
        <ChargeTransactionPayment<T> as SignedExtension>::post_dispatch(
            maybe_pre, info, post_info, len, result,
        )
    }
}

impl<T: Config> CustomChargeTransactionPayment<T>
where
    CallOf<T>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
    fn pre_dispatch_info<'a>(
        call: &'a <T as frame_system::Config>::RuntimeCall,
        info: &'a DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
    ) -> Cow<'a, DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>> {
        // If the call is not subject to fee multiplication, divide weight by fee multiplier.
        // This action will effectively be cancelled out at the time the fee is calculated.
        //
        // TODO: consider reimplementing.
        // This procedure does introduce a rounding error 𝞮 =  w - ⎣w / m⎦⋅m
        // However, we argue that such error is negligible:
        // - the rounding error can never exceed `m` (multiplier). Order of `w` (weight) is
        // usually not less than 10^8 while the fee multiplier should not be greater than 10^3.
        // Therefore the rounding error shouln't exceed 0.001% in the worst case.
        // Note: this only applies to calls that do not affect message queue, that is are
        // relatively rare. Still, a better solution can be found.
        if !T::ExtraFeeCallFilter::contains(call) {
            let multiplier = TransactionPayment::<T>::next_fee_multiplier();
            if multiplier > Multiplier::saturating_from_integer(1) {
                let mut info: DispatchInfo = *info;
                info.weight = Weight::from_ref_time(
                    multiplier
                        .reciprocal() // take inverse
                        .unwrap_or_else(Multiplier::max_value)
                        .saturating_mul_int(info.weight.ref_time()),
                );
                Cow::Owned(info)
            } else {
                Cow::Borrowed(info)
            }
        } else {
            Cow::Borrowed(info)
        }
    }
}

/// Custom fee multiplier which looks at the message queue size to increase weight fee
pub struct GearFeeMultiplier<T, S>(sp_std::marker::PhantomData<(T, S)>);

impl<T, S> Convert<Multiplier, Multiplier> for GearFeeMultiplier<T, S>
where
    T: Config,
    S: Get<u128>,
{
    fn convert(_previous: Multiplier) -> Multiplier {
        let len_step = S::get().max(1); // Avoiding division by 0.

        let queue_len: u128 = QueueOf::<T>::len().saturated_into();
        let pow = queue_len.saturating_div(len_step);
        Multiplier::saturating_from_integer(1 << pow)
    }
}

impl<T, S> MultiplierUpdate for GearFeeMultiplier<T, S>
where
    T: Config,
    S: Get<u128>,
{
    fn max() -> Multiplier {
        Default::default()
    }
    fn min() -> Multiplier {
        Default::default()
    }
    fn target() -> Perquintill {
        Default::default()
    }
    fn variability() -> Multiplier {
        Default::default()
    }
}

impl<T: Config> Pallet<T> {
    /// Modification of the `pallet_transaction_payment::Pallet<T>::query_info()`
    /// that is aware of the transaction fee customization based on a specific call
    pub fn query_info<
        Extrinsic: sp_runtime::traits::Extrinsic + GetDispatchInfo + ExtractCall<CallOf<T>>,
    >(
        unchecked_extrinsic: Extrinsic,
        len: u32,
    ) -> RuntimeDispatchInfo<BalanceOf<T>>
    where
        CallOf<T>: Dispatchable<Info = DispatchInfo>,
        BalanceOf<T>: FixedPointOperand,
    {
        let DispatchInfo {
            weight,
            class,
            pays_fee,
        } = <Extrinsic as GetDispatchInfo>::get_dispatch_info(&unchecked_extrinsic);

        let partial_fee = if unchecked_extrinsic.is_signed().unwrap_or(false) {
            let call: CallOf<T> =
                <Extrinsic as ExtractCall<CallOf<T>>>::extract_call(&unchecked_extrinsic);
            // If call is exempted from weight multiplication pre-divide it with the fee multiplier
            let adjusted_weight = if !T::ExtraFeeCallFilter::contains(&call) {
                Weight::from_ref_time(
                    TransactionPayment::<T>::next_fee_multiplier()
                        .reciprocal()
                        .unwrap_or_else(Multiplier::max_value)
                        .saturating_mul_int(weight.ref_time()),
                )
            } else {
                weight
            };
            TransactionPayment::<T>::compute_fee(
                len,
                &DispatchInfo {
                    weight: adjusted_weight,
                    class,
                    pays_fee,
                },
                0u32.into(),
            )
        } else {
            // Unsigned extrinsics have no partial fee.
            0u32.into()
        };

        RuntimeDispatchInfo {
            weight,
            class,
            partial_fee,
        }
    }

    /// Modification of the `pallet_transaction_payment::Pallet<T>::query_fee_details()`
    pub fn query_fee_details<
        Extrinsic: sp_runtime::traits::Extrinsic + GetDispatchInfo + ExtractCall<CallOf<T>>,
    >(
        unchecked_extrinsic: Extrinsic,
        len: u32,
    ) -> FeeDetails<BalanceOf<T>>
    where
        CallOf<T>: Dispatchable<Info = DispatchInfo>,
        BalanceOf<T>: FixedPointOperand,
    {
        let DispatchInfo {
            weight,
            class,
            pays_fee,
        } = <Extrinsic as GetDispatchInfo>::get_dispatch_info(&unchecked_extrinsic);

        let tip = 0u32.into();

        if unchecked_extrinsic.is_signed().unwrap_or(false) {
            let call: CallOf<T> =
                <Extrinsic as ExtractCall<CallOf<T>>>::extract_call(&unchecked_extrinsic);
            let adjusted_weight = if !T::ExtraFeeCallFilter::contains(&call) {
                Weight::from_ref_time(
                    TransactionPayment::<T>::next_fee_multiplier()
                        .reciprocal()
                        .unwrap_or_else(Multiplier::max_value)
                        .saturating_mul_int(weight.ref_time()),
                )
            } else {
                weight
            };
            TransactionPayment::<T>::compute_fee_details(
                len,
                &DispatchInfo {
                    weight: adjusted_weight,
                    class,
                    pays_fee,
                },
                tip,
            )
        } else {
            // Unsigned extrinsics have no inclusion fee.
            FeeDetails {
                inclusion_fee: None,
                tip,
            }
        }
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_transaction_payment::Config {
        /// Filter for calls subbject for extra fees
        type ExtraFeeCallFilter: Contains<CallOf<Self>>;

        /// Type representing message queue
        type Messenger: Messenger<Capacity = u32>;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);
}
