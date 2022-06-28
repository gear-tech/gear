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

use super::*;
use core::cmp::Ordering;

/// Opaque, move-only struct with private field to denote that value has been destroyed
/// without any equal and opposite accounting.
#[derive(RuntimeDebug, PartialEq, Eq)]
pub struct NegativeImbalance<Balance: BalanceTrait, TotalValue: ValueStorage<Value = Balance>>(
    Balance,
    PhantomData<TotalValue>,
);

impl<Balance: BalanceTrait, TotalValue: ValueStorage<Value = Balance>>
    NegativeImbalance<Balance, TotalValue>
{
    /// Create a new negative imbalance from value amount.
    pub fn new(amount: Balance) -> Self {
        NegativeImbalance(amount, PhantomData)
    }
}

impl<Balance: BalanceTrait, TotalValue: ValueStorage<Value = Balance>> TryDrop
    for NegativeImbalance<Balance, TotalValue>
{
    fn try_drop(self) -> Result<(), Self> {
        self.drop_zero()
    }
}

impl<Balance: BalanceTrait, TotalValue: ValueStorage<Value = Balance>> Default
    for NegativeImbalance<Balance, TotalValue>
{
    fn default() -> Self {
        Self::zero()
    }
}

impl<Balance: BalanceTrait, TotalValue: ValueStorage<Value = Balance>> Imbalance<Balance>
    for NegativeImbalance<Balance, TotalValue>
{
    type Opposite = PositiveImbalance<Balance, TotalValue>;

    fn zero() -> Self {
        Self(Zero::zero(), PhantomData)
    }

    fn drop_zero(self) -> Result<(), Self> {
        if self.0.is_zero() {
            Ok(())
        } else {
            Err(self)
        }
    }

    fn split(self, amount: Balance) -> (Self, Self) {
        let first = self.0.min(amount);
        let second = self.0 - first;

        mem::forget(self);
        (Self(first, PhantomData), Self(second, PhantomData))
    }

    fn merge(mut self, other: Self) -> Self {
        self.0 = self.0.saturating_add(other.0);
        mem::forget(other);

        self
    }

    fn subsume(&mut self, other: Self) {
        self.0 = self.0.saturating_add(other.0);
        mem::forget(other);
    }

    fn offset(self, other: Self::Opposite) -> SameOrOther<Self, Self::Opposite> {
        let (a, b) = (self.0, other.peek());
        mem::forget((self, other));

        match a.cmp(&b) {
            Ordering::Less => SameOrOther::Other(PositiveImbalance::new(b - a)),
            Ordering::Equal => SameOrOther::None,
            Ordering::Greater => SameOrOther::Same(Self(a - b, PhantomData)),
        }
    }

    fn peek(&self) -> Balance {
        self.0
    }
}

impl<Balance: BalanceTrait, TotalValue: ValueStorage<Value = Balance>> Drop
    for NegativeImbalance<Balance, TotalValue>
{
    /// Basic drop handler will just square up the total issuance.
    fn drop(&mut self) {
        TotalValue::mutate(|v| {
            let new_value = v.unwrap_or_else(Zero::zero);
            if self.0 > new_value {
                log::debug!(
                    target: "essential",
                    "Unaccounted gas detected: burnt {:?}, known total supply was {:?}.",
                    self.0,
                    *v
                )
            }

            *v = Some(new_value.saturating_sub(self.0));
        });
    }
}
