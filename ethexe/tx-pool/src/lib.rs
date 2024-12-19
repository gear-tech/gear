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
    new, InputTask, OutputTask, TxPoolInputTaskSender, TxPoolInstantiationArtifacts,
    TxPoolOutputTaskReceiver, TxPoolService,
};
pub use transaction::{
    EthexeTransaction, RawEthexeTransacton, SignedEthexeTransaction, Transaction, TxHashBlake2b256,
    TxReferenceBlockHash, TxSignature,
};

use service::TxPoolOutputTaskSender;
use validation::{TxValidator, TxValidatorFinishResult};

/// Transaction pool service with a [`EthexeTransaction`] transaction type and a [`StandardTxPool`] as a transaction pool.
pub type StandardTxPoolService = TxPoolService<SignedEthexeTransaction>;
/// Transaction pool input task sender with a [`EthexeTransaction`] transaction type.
pub type StandardInputTaskSender = TxPoolInputTaskSender<SignedEthexeTransaction>;
/// Transaction pool output task receiver with a [`EthexeTransaction`] transaction type.
pub type StandardOutputTaskReceiver = TxPoolOutputTaskReceiver<SignedEthexeTransaction>;
/// Transaction pool instantiation artifacts with a [`EthexeTransaction`] transaction type and a [`StandardTxPool`] as a transaction pool.
pub type StandardTxPoolInstantiationArtifacts =
    TxPoolInstantiationArtifacts<SignedEthexeTransaction>;
