// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Ethexe transaction pool.

mod service;
mod transaction;
mod validation;

#[cfg(test)]
mod tests;

pub use service::{
    new, InputTask, OutputTask, TxPoolEvent, TxPoolInputTaskSender, TxPoolKit,
    TxPoolOutputTaskReceiver, TxPoolService,
};
pub use transaction::{RawTransacton, SignedTransaction, Transaction};

use validation::TxValidator;

/// Transaction pool input task sender with a [`SignedEthexeTransaction`] transaction type.
pub type TxPoolSender = TxPoolInputTaskSender<SignedTransaction>;
/// Transaction pool output task receiver with a [`SignedEthexeTransaction`] transaction type.
pub type TxPoolReceiver = TxPoolOutputTaskReceiver<SignedTransaction>;
