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

mod validation;

#[cfg(test)]
mod tests;

pub use ethexe_common::tx_pool::{RawTransacton, SignedTransaction, Transaction};

use anyhow::{Context as _, Result};
use ethexe_db::Database;
use ethexe_signer::{Address, Signature, ToDigest};
use futures::{
    ready,
    stream::{FusedStream, Stream},
};
use gprimitives::H160;
use parity_scale_codec::Encode;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;
use validation::TxValidator;

/// Transaction pool service.
///
/// Serves as an interface for the transaction pool core.
pub struct TxPoolService {
    db: Database,
    // No concurrent access for ready_tx is here,
    // so no need for the mutex.
    ready_tx: VecDeque<SignedTransaction>,
    readiness_sender: mpsc::UnboundedSender<()>,
    readiness_receiver: mpsc::UnboundedReceiver<()>,
}

impl TxPoolService {
    pub fn new(db: Database) -> Self {
        let (readiness_sender, readiness_receiver) = mpsc::unbounded_channel();
        Self {
            db,
            ready_tx: VecDeque::new(),
            readiness_sender,
            readiness_receiver,
        }
    }

    /// Basically validates the transaction and includes the transaction
    /// to the ready queue, so it's returned by the service stream.
    pub fn process(&mut self, transaction: SignedTransaction) -> Result<SignedTransaction> {
        TxValidator::new(transaction, self.db.clone())
            .with_all_checks()
            .validate()
            .context("tx validation failed")
            .inspect(|validated_tx| {
                self.ready_tx.push_back(validated_tx.clone());
                self.readiness_sender
                    .send(())
                    .expect("receiver is always alive");
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxPoolEvent {
    PropogateTransaction(SignedTransaction),
}

impl Stream for TxPoolService {
    type Item = TxPoolEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        ready!(self.readiness_receiver.poll_recv(cx)).expect("sender is always alive");

        let ret = self.ready_tx.pop_front();

        // Readiness receiver is changed only when a new transaction is pushed to the ready_tx queue
        debug_assert!(ret.is_some());
        Poll::Ready(ret.map(TxPoolEvent::PropogateTransaction))
    }
}

impl FusedStream for TxPoolService {
    fn is_terminated(&self) -> bool {
        false
    }
}

/// Gets source of the `SendMessage` transaction recovering it from the signature.
pub fn tx_send_message_source(tx: &SignedTransaction) -> Result<H160> {
    Signature::try_from(tx.signature.as_ref())
        .and_then(|signature| signature.recover_from_digest(tx.transaction.encode().to_digest()))
        .map(|public_key| H160::from(Address::from(public_key).0))
}

/// Ethexe transaction signature.
fn tx_signature(tx: &SignedTransaction) -> Result<Signature> {
    Signature::try_from(tx.signature.as_ref())
}
