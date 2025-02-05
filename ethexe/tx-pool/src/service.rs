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

//! Transaction pool service and io.

pub use input::{InputTask, TxPoolInputTaskSender};
pub use output::{OutputTask, TxPoolOutputTaskReceiver};

pub(crate) use output::TxPoolOutputTaskSender;

use crate::{SignedTransaction, TxValidator};
use anyhow::{Context as _, Result};
use ethexe_db::Database;
use futures::{
    ready,
    stream::{FusedStream, Stream},
    FutureExt,
};
use gprimitives::H256;
use input::TxPoolInputTaskReceiver;
use parity_scale_codec::Encode;
use std::{
    collections::VecDeque,
    future::Future,
    pin::{pin, Pin},
    task::{Context, Poll},
};
use tokio::sync::{mpsc, oneshot, watch, Mutex};

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
            .map(|validated_tx| {
                self.ready_tx.push_back(validated_tx.clone());
                self.readiness_sender
                    .send(())
                    .expect("receiver is always alive");

                validated_tx
            })
    }
}

// TODO [sab] test case
// process, process, poll_next, poll_next, poll_next, process, poll_next
// TODO [sab] do we really need the service? we can just propagate the transaction
// after the TxPoolService::process
#[derive(Debug, Clone)]
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

mod input {
    use anyhow::Result;
    use gprimitives::H256;
    use std::ops::{Deref, DerefMut};
    use tokio::sync::{mpsc, oneshot};

    /// Input task for the transaction pool service.
    pub enum InputTask<Tx> {
        /// Request for checking the transaction validity.
        ValidatePreDispatch {
            transaction: Tx,
            response_sender: oneshot::Sender<Result<Tx>>,
        },
        /// Request for adding the transaction to the transaction pool.
        /// Sends the response back to the task sender, if there's receiver,
        /// that expects the response.
        AddTransaction {
            transaction: Tx,
            response_sender: Option<oneshot::Sender<Result<H256>>>,
        },
    }

    /// Transaction pool input task sender.
    ///
    /// Used as a sending end to communicate with the transaction pool service
    /// to run some action on the transaction pool.
    #[derive(Debug, Clone)]
    pub struct TxPoolInputTaskSender<Tx> {
        pub(crate) sender: mpsc::UnboundedSender<InputTask<Tx>>,
    }

    impl<Tx> Deref for TxPoolInputTaskSender<Tx> {
        type Target = mpsc::UnboundedSender<InputTask<Tx>>;

        fn deref(&self) -> &Self::Target {
            &self.sender
        }
    }

    impl<Tx> DerefMut for TxPoolInputTaskSender<Tx> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.sender
        }
    }

    /// Transaction pool input task receiver.
    pub(crate) struct TxPoolInputTaskReceiver<Tx> {
        pub(crate) receiver: mpsc::UnboundedReceiver<InputTask<Tx>>,
    }

    impl<Tx> Deref for TxPoolInputTaskReceiver<Tx> {
        type Target = mpsc::UnboundedReceiver<InputTask<Tx>>;

        fn deref(&self) -> &Self::Target {
            &self.receiver
        }
    }

    impl<Tx> DerefMut for TxPoolInputTaskReceiver<Tx> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.receiver
        }
    }
}

mod output {
    use std::ops::{Deref, DerefMut};
    use tokio::sync::mpsc;

    /// Output task sent from the transaction pool service.
    ///
    /// The task is not obligatory to be anyhow handled,
    /// but is a way to communicate with an external service.
    #[derive(Debug)]
    pub enum OutputTask<Tx> {
        /// Requests for a transcation to be propogated.
        PropogateTransaction { transaction: Tx },
    }

    /// Transaction pool output task sender.
    #[derive(Debug, Clone)]
    pub(crate) struct TxPoolOutputTaskSender<Tx> {
        pub(crate) sender: mpsc::UnboundedSender<OutputTask<Tx>>,
    }

    impl<Tx> Deref for TxPoolOutputTaskSender<Tx> {
        type Target = mpsc::UnboundedSender<OutputTask<Tx>>;

        fn deref(&self) -> &Self::Target {
            &self.sender
        }
    }

    impl<Tx> DerefMut for TxPoolOutputTaskSender<Tx> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.sender
        }
    }

    /// Transaction pool output task receiver.
    ///
    /// Used as a receiving end to transaction pool service
    /// external communication channel.
    pub struct TxPoolOutputTaskReceiver<Tx> {
        pub(crate) receiver: mpsc::UnboundedReceiver<OutputTask<Tx>>,
    }

    impl<Tx> Deref for TxPoolOutputTaskReceiver<Tx> {
        type Target = mpsc::UnboundedReceiver<OutputTask<Tx>>;

        fn deref(&self) -> &Self::Target {
            &self.receiver
        }
    }

    impl<Tx> DerefMut for TxPoolOutputTaskReceiver<Tx> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.receiver
        }
    }
}
