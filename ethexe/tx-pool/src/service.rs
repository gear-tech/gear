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
use anyhow::Result;
use ethexe_db::Database;
use futures::{
    ready,
    stream::{FusedStream, Stream},
};
use gprimitives::H256;
use input::TxPoolInputTaskReceiver;
use parity_scale_codec::Encode;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::{mpsc, Mutex};

/// Creates a new transaction pool service.
pub fn new(db: Database) -> TxPoolKit {
    let (tx_in, rx_in) = mpsc::unbounded_channel();
    let (tx_out, rx_out) = mpsc::unbounded_channel();

    let service = TxPoolService {
        db,
        ready_txs: Mutex::new(VecDeque::new()),
    };

    TxPoolKit {
        service,
        tx_pool_sender: TxPoolInputTaskSender { sender: tx_in },
        tx_pool_receiver: TxPoolOutputTaskReceiver { receiver: rx_out },
    }
}

/// Transaction pool kit, which consists of the pool service and channels to communicate with it.
pub struct TxPoolKit {
    pub service: TxPoolService,
    pub tx_pool_sender: TxPoolInputTaskSender<SignedTransaction>,
    pub tx_pool_receiver: TxPoolOutputTaskReceiver<SignedTransaction>,
}

/// Transaction pool service.
///
/// Serves as an interface for the transaction pool core.
pub struct TxPoolService {
    db: Database,
    ready_txs: Mutex<VecDeque<SignedTransaction>>,
}

impl TxPoolService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            ready_txs: Mutex::new(VecDeque::new()),
        }
    }

    pub async fn add_transaction(&self, transaction: SignedTransaction) -> Result<H256> {
        /*
        Possibly:
        1. validate transaction
        2. add to DB
        3. start execution of the transaction on a separate thread.
        4. if valid return the tx hash
        */

        todo!()
    }

    /// Runs transaction pool service expecting to receive tasks from the
    /// tx pool input task sender.
    // pub async fn run(mut self) {
    //     // Finishes working of all the input task senders are dropped.
    //     while let Some(task) = self.receiver.recv().await {
    //         match task {
    //             InputTask::ValidatePreDispatch {
    //                 transaction,
    //                 response_sender,
    //             } => {
    //                 // No need for a uniqueness check as the input task is sent on existing transactions
    //                 debug_assert!(self
    //                     .db
    //                     .validated_transaction(transaction.tx_hash())
    //                     .is_some());

    //                 let res = TxValidator::new(transaction, self.db.clone())
    //                     .with_signature_check()
    //                     .with_mortality_check()
    //                     .validate();
    //                 let _ = response_sender.send(res).inspect_err(|_| {
    //                     // No panic case as the request itself is going to be executed.
    //                     // The dropped receiver signalizes that the external task sender
    //                     // has crashed or is malformed, so problems should be handled there.
    //                     log::error!("`ValidateTransaction` task receiver is stopped or dropped.");
    //                 });
    //             }
    //             InputTask::AddTransaction {
    //                 transaction,
    //                 response_sender,
    //             } => {
    //                 let res = self.validate_tx_full(transaction).map(|tx| {
    //                     let tx_hash = tx.tx_hash();
    //                     let tx_encoded = tx.encode();

    //                     // Request the external service for the tx propagation.
    //                     self.sender.send(OutputTask::PropogateTransaction {
    //                         transaction: tx.clone(),
    //                     }).unwrap_or_else(|e| {
    //                         // If receiving end of the external service is dropped, it's a panic case,
    //                         // because otherwise transaction processing can't be performed correctly.
    //                         let err_msg = format!(
    //                             "Failed to send `PropogateTransaction` task. External service receiving end \
    //                             might have been dropped. Got an error: {e:?}."
    //                         );

    //                         log::error!("{err_msg}");
    //                         panic!("{err_msg}");
    //                     });

    //                     // Store the validated transaction to the database.
    //                     self.db.set_validated_transaction(tx_hash, tx_encoded);

    //                     // Start transaction execution
    //                     tokio::spawn(Self::execute_transaction(self.db.clone(), tx));

    //                     tx_hash
    //                 });

    //                 if let Some(response_sender) = response_sender {
    //                     let _ = response_sender.send(res).inspect_err(|_| {
    //                         // No panic case as a responsibility of transaction piil is fulfilled.
    //                         // The dropped receiver signalizes that the external task sender
    //                         // has crashed or is malformed, so problems should be handled there.
    //                         log::error!("`AddTransaction` task receiver is stopped or dropped.")
    //                     });
    //                 }
    //             }
    //         }
    //     }
    // }

    fn validate_tx_full(&self, transaction: SignedTransaction) -> Result<SignedTransaction> {
        TxValidator::new(transaction, self.db.clone())
            .with_all_checks()
            .validate()
    }

    async fn execute_transaction(db: Database, _transaction: SignedTransaction) {
        let _processor = ethexe_processor::Processor::new(db.clone());
        // TODO (breathx) Execute transaction
        // TODO (braethx) Remove transaction from the database.
        log::warn!("Unimplemented transaction execution");
    }
}

#[derive(Debug, Clone)]
pub enum TxPoolEvent {
    PropogateTransaction(SignedTransaction),
}

impl Stream for TxPoolService {
    type Item = TxPoolEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mutex_lock_fut = self.ready_txs.lock();
        tokio::pin!(mutex_lock_fut);

        let mut ready_txs = ready!(mutex_lock_fut.poll(cx));

        Poll::Ready(ready_txs.pop_front().map(TxPoolEvent::PropogateTransaction))
    }
}

impl FusedStream for TxPoolService {
    // TODO [sab] yet
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
