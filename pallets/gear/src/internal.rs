// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Internal details of Gear Pallet implementation.

use crate::{Authorship, BalanceOf, Config, CurrencyOf, GasHandlerOf, Pallet};
use common::{GasPrice, GasProvider, GasTree, Origin};
use frame_support::traits::{
    BalanceStatus, Currency, ExistenceRequirement, Imbalance, ReservableCurrency,
};
use gear_core::ids::MessageId;
use sp_runtime::traits::Zero;

impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    // TODO: Consider usage of `Balance` instead of gas conversions.
    // TODO: Consider using here some tolerance. Missed due to identity fee.
    // TODO: If tolerance applied, consider unreserve excess funds, while
    // converting gas into value.
    /// Moves reserved funds from account to freed funds of another account.
    ///
    /// Repatriates reserved if both accounts exist,
    /// uses direct transfer otherwise.
    pub(crate) fn transfer_reserved(from: &T::AccountId, to: &T::AccountId, value: BalanceOf<T>) {
        // If destination account can reserve minimum balance, it means that
        // account exists and can receive repatriation of reserved funds.
        //
        // Otherwise need to transfer them directly.

        // Querying minimum balance (existential deposit).
        let existential_deposit = CurrencyOf::<T>::minimum_balance();

        // Checking balance existence of destination address.
        if CurrencyOf::<T>::can_reserve(to, existential_deposit) {
            // Repatriating reserved to existent account.
            let unrevealed =
                CurrencyOf::<T>::repatriate_reserved(from, to, value, BalanceStatus::Free)
                    .unwrap_or_else(|e| {
                        unreachable!("Failed to repatriate reserved funds: {:?}", e)
                    });

            // Validating unrevealed funds after repatriation.
            if !unrevealed.is_zero() {
                unreachable!("Reserved funds wasn't fully repatriated.")
            }
        } else {
            // Unreserving funds from sender to transfer them directly.
            let unrevealed = CurrencyOf::<T>::unreserve(from, value);

            // Validating unrevealed funds after unreserve.
            if !unrevealed.is_zero() {
                unreachable!("Not all requested value was unreserved");
            }

            // Transfer to inexistent account.
            CurrencyOf::<T>::transfer(from, to, value, ExistenceRequirement::AllowDeath)
                .unwrap_or_else(|e| unreachable!("Failed to transfer value: {:?}", e));
        }
    }

    /// Spends given amount of gas from given `MessageId` in `GasTree`.
    ///
    /// Represents logic of burning gas by transferring gas from
    /// current `GasTree` owner to actual block producer.
    pub(crate) fn spend_gas(
        message_id: MessageId,
        amount: <T::GasProvider as GasProvider>::Balance,
    ) {
        // Spending gas amount from `GasNode`.
        // Here is a negative imbalance. Used `_` to force drop in place.
        let _ = GasHandlerOf::<T>::spend(message_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Querying external id. Fails in cases of `GasTree` invalidations.
        let optional_external = GasHandlerOf::<T>::get_external(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // External id may not be found only for inexistent node.
        let external =
            optional_external.unwrap_or_else(|| unreachable!("Non existent GasNode queried"));

        // Querying actual block author to reward.
        let block_author = Authorship::<T>::author()
            .unwrap_or_else(|| unreachable!("Failed to find block author!"));

        // Converting gas amount into value.
        let value = T::GasPrice::gas_price(amount);

        // Transferring reserved funds from external to block author.
        Self::transfer_reserved(&external, &block_author, value);
    }

    /// Consumes message by given `MessageId`.
    ///
    /// Updates currency and balances data on imbalance creation.
    ///
    /// SAFETY NOTE: calls `unreachable!()` in cases of `GasHandler::consume`
    /// errors or on non-zero unrevealed balances in `Currency::unreserve`.
    pub(crate) fn consume_message(message_id: MessageId) {
        // Consuming `GasNode`, returning optional outcome with imbalance.
        let outcome = GasHandlerOf::<T>::consume(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        // Unreserving funds, if imbalance returned.
        if let Some((imbalance, external)) = outcome {
            // Peeking numeric value from negative imbalance.
            let gas_left = imbalance.peek();

            // Unreserving funds, if left non-zero amount of gas.
            if !gas_left.is_zero() {
                log::debug!("Unreserve on message consumed: {gas_left} to {external:?}");

                // Converting gas amount into value.
                let value = T::GasPrice::gas_price(gas_left);

                // Unreserving funds.
                let unrevealed = CurrencyOf::<T>::unreserve(&external, value);

                // Validating unrevealed funds after unreserve.
                if !unrevealed.is_zero() {
                    unreachable!("Not all requested value was unreserved");
                }
            }
        }
    }
}
