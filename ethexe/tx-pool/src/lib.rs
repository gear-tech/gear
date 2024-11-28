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

#[cfg(test)]
mod tests;

pub use service::{
    new, InputTask, OutputTask, TxPoolInputTaskSender, TxPoolInstantiationArtifacts,
    TxPoolOutputTaskReceiver, TxPoolService,
};
pub use transaction::{EthexeTransaction, Transaction};

use anyhow::Result;
use ethexe_db::Database;
use parity_scale_codec::Encode;
use std::marker::PhantomData;

/// Transaction pool with a [`EthexeTransaction`] transaction type.
pub type StandardTxPool = TxPoolCore<EthexeTransaction>;
/// Transaction pool service with a [`EthexeTransaction`] transaction type and a [`StandardTxPool`] as a transaction pool.
pub type StandardTxPoolService = TxPoolService<EthexeTransaction, StandardTxPool>;
/// Transaction pool input task sender with a [`EthexeTransaction`] transaction type.
pub type StandardInputTaskSender = TxPoolInputTaskSender<EthexeTransaction>;
/// Transaction pool output task receiver with a [`EthexeTransaction`] transaction type.
pub type StandardOutputTaskReceiver = TxPoolOutputTaskReceiver<EthexeTransaction>;
/// Transaction pool instantiation artifacts with a [`EthexeTransaction`] transaction type and a [`StandardTxPool`] as a transaction pool.
pub type StandardTxPoolInstantiationArtifacts =
    TxPoolInstantiationArtifacts<EthexeTransaction, StandardTxPool>;

/// Transaction pool trait.
pub trait TxPoolTrait {
    /// Transaction type.
    type Transaction: Transaction;

    /// Add transaction to the pool.
    fn add_transaction(&self, transaction: Self::Transaction) -> Result<()>;
}

impl TxPoolTrait for () {
    type Transaction = ();

    fn add_transaction(&self, _transaction: Self::Transaction) -> Result<()> {
        Ok(())
    }
}

pub struct TxPoolCore<Tx> {
    db: Database,
    _phantom: PhantomData<Tx>,
}

impl<Tx> TxPoolCore<Tx> {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            _phantom: PhantomData,
        }
    }
}

impl<Tx> TxPoolTrait for TxPoolCore<Tx>
where
    Tx: Transaction + Encode,
    Tx::Error: Into<anyhow::Error>,
{
    type Transaction = Tx;

    fn add_transaction(&self, transaction: Self::Transaction) -> Result<()> {
        let tx_bytes = transaction.encode();
        let tx_hash = transaction.tx_hash();

        if self.db.validated_transaction(tx_hash).is_none() {
            transaction.validate().map_err(Into::into)?;
            self.db.set_validated_transaction(tx_hash, tx_bytes);
        }

        Ok(())
    }
}

impl<Tx> From<(Database,)> for TxPoolCore<Tx> {
    fn from((db,): (Database,)) -> Self {
        TxPoolCore::new(db)
    }
}
