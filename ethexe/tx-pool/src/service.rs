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

//! Transaction pool io.

pub use input::{InputTask, TxPoolInputTaskSender};
pub use output::{OutputTask, TxPoolOutputTaskReceiver};

pub(crate) use output::TxPoolOutputTaskSender;

use crate::{Transaction, TxValidator, TxValidatorFinishResult};
use ethexe_db::Database;
use input::TxPoolInputTaskReceiver;
use tokio::sync::mpsc;

/// Creates a new transaction pool service.
pub fn new<Tx>(db: Database) -> TxPoolInstantiationArtifacts<Tx>
where
    Tx: Transaction + Send + Sync + 'static,
{
    let (tx_in, rx_in) = mpsc::unbounded_channel();
    let (tx_out, rx_out) = mpsc::unbounded_channel();

    let service = TxPoolService {
        db,
        input_interface: TxPoolInputTaskReceiver { receiver: rx_in },
        output_inteface: TxPoolOutputTaskSender { sender: tx_out },
    };

    TxPoolInstantiationArtifacts {
        service,
        input_sender: TxPoolInputTaskSender { sender: tx_in },
        output_receiver: TxPoolOutputTaskReceiver { receiver: rx_out },
    }
}

/// Transaction pool instantiation artifacts carrier.
pub struct TxPoolInstantiationArtifacts<Tx: Transaction> {
    pub service: TxPoolService<Tx>,
    pub input_sender: TxPoolInputTaskSender<Tx>,
    pub output_receiver: TxPoolOutputTaskReceiver<Tx>,
}

/// Transaction pool service.
///
/// Serves as an interface for the transaction pool core.
pub struct TxPoolService<Tx: Transaction> {
    db: Database,
    input_interface: TxPoolInputTaskReceiver<Tx>,
    output_inteface: TxPoolOutputTaskSender<Tx>,
}

impl<Tx: Transaction + Send + Sync + 'static> TxPoolService<Tx> {
    /// Runs transaction pool service expecting to receive tasks from the
    /// tx pool input task sender.
    pub async fn run(mut self) {
        while let Some(task) = self.input_interface.recv().await {
            match task {
                InputTask::AddTransaction {
                    transaction,
                    response_sender,
                } => {
                    let res = TxValidator::new(transaction, self.db.clone())
                        .with_signature_check()
                        .with_mortality_check()
                        .with_uniqueness_check()
                        .with_executable_tx_check(self.output_inteface.clone())
                        .full_validate()
                        .await
                        .finish_validator_res()
                        .map(|tx| {
                            let tx_hash = tx.tx_hash();
                            let tx_encoded = tx.encode();

                            // Request propagation.
                            if let Err(err) =
                                self.output_inteface.send(OutputTask::PropogateTransaction {
                                    transaction: tx.clone(),
                                })
                            {
                                // TODO [sab] handle properly
                                log::error!("Failed to send `PropogateTransaction` task: {err:?}");
                            }

                            // Request execution.
                            if let Err(err) = self
                                .output_inteface
                                .send(OutputTask::ExecuteTransaction { transaction: tx })
                            {
                                // TODO [sab] handle properly
                                log::error!("Failed to send `PropogateTransaction` task: {err:?}");
                            }

                            self.db.set_validated_transaction(tx_hash, tx_encoded);

                            tx_hash
                        });

                    if let Some(response_sender) = response_sender {
                        let _ = response_sender.send(res).inspect_err(|err| {
                            // TODO [sab] handle properly
                            log::error!("`AddTransaction` task receiver dropped - {err:?}")
                        });
                    }
                }
            }
        }
    }
}

mod input {
    use anyhow::Result;
    use gprimitives::H256;
    use std::ops::{Deref, DerefMut};
    use tokio::sync::{mpsc, oneshot};

    /// Input task for the transaction pool service.
    ///
    /// The task is later processed to be executed by
    /// the [`crate::TxPool`] implementation.
    pub enum InputTask<Tx> {
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
    use tokio::sync::{mpsc, oneshot};

    /// Output task sent from the transaction pool service.
    ///
    /// The task is not obligatory to be anyhow handled,
    /// but is a way to communicate with an external service.
    #[derive(Debug)]
    pub enum OutputTask<Tx> {
        /// Requests for a transcation to propogation.
        PropogateTransaction { transaction: Tx },
        /// Requests for a check by external service that transaction is executable.
        CheckIsExecutable {
            transaction: Tx,
            response_sender: oneshot::Sender<bool>,
        },
        /// Requests for a tx to be executed.
        ExecuteTransaction { transaction: Tx },
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
