// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! Gearexe transaction pool.

mod validation;

#[cfg(test)]
mod tests;

use anyhow::{Context as _, Result};
pub use gearexe_common::tx_pool::{
    OffchainTransaction, RawOffchainTransaction, SignedOffchainTransaction,
};
use gearexe_db::Database;
use futures::{Stream, stream::FusedStream};
use gprimitives::{ActorId, H160, H256};
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use validation::TxValidator;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TxPoolEvent {
    PublishOffchainTransaction(SignedOffchainTransaction),
}

/// Transaction pool service.
///
/// Serves as an interface for the transaction pool core.
pub struct TxPoolService {
    db: Database,
    events: VecDeque<TxPoolEvent>,
    waker: Option<Waker>,
}

impl TxPoolService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            events: VecDeque::new(),
            waker: None,
        }
    }

    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    /// Basically validates the transaction and includes the transaction
    /// to the ready queue, so it's returned by the service stream.
    fn validate(
        &self,
        transaction: SignedOffchainTransaction,
    ) -> Result<SignedOffchainTransaction> {
        TxValidator::new(transaction, self.db.clone())
            .with_all_checks()
            .validate()
            .context("Tx validation failed")
    }

    pub fn process_offchain_transaction(
        &mut self,
        transaction: SignedOffchainTransaction,
    ) -> Result<H256> {
        let validated_tx = self
            .validate(transaction)
            .context("Failed to validate offchain transaction")?;
        let tx_hash = validated_tx.tx_hash();

        // Set valid transaction
        self.db.set_offchain_transaction(validated_tx.clone());

        // Propagate transaction
        self.events
            .push_back(TxPoolEvent::PublishOffchainTransaction(validated_tx));
        self.wake();

        // TODO (breathx) Execute transaction
        log::info!("Unimplemented tx execution");

        Ok(tx_hash)
    }
}

impl Stream for TxPoolService {
    type Item = TxPoolEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(Some(event));
        }

        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl FusedStream for TxPoolService {
    fn is_terminated(&self) -> bool {
        false
    }
}

/// Gets source of the `SendMessage` transaction recovering it from the signature.
pub fn tx_send_message_source(tx: &SignedOffchainTransaction) -> ActorId {
    H160(tx.public_key().to_address().0).into()
}
