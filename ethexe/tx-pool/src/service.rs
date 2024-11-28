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

use crate::{Transaction, TxPoolTrait};
use input::TxPoolInputTaskReceiver;
use output::TxPoolOutputTaskSender;
use tokio::sync::mpsc;

/// Transaction pool instantiation artifacts carrier.
pub struct TxPoolInstantiationArtifacts<Tx: Transaction, TxPool: TxPoolTrait<Transaction = Tx>> {
    pub service: TxPoolService<Tx, TxPool>,
    pub input_sender: TxPoolInputTaskSender<Tx>,
    pub output_receiver: TxPoolOutputTaskReceiver<Tx>,
}

/// Transaction pool service.
///
/// Serves as an interface for the transaction pool core.
pub struct TxPoolService<Tx: Transaction, TxPool: TxPoolTrait<Transaction = Tx>> {
    core: TxPool,
    input_interface: TxPoolInputTaskReceiver<Tx>,
    output_inteface: TxPoolOutputTaskSender<Tx>,
}

impl<Tx: Transaction + Clone, TxPool: TxPoolTrait<Transaction = Tx>> TxPoolService<Tx, TxPool> {
    pub fn new(tx_pool_core: impl Into<TxPool>) -> TxPoolInstantiationArtifacts<Tx, TxPool> {
        let tx_pool_core = tx_pool_core.into();
        let (tx_in, rx_in) = mpsc::unbounded_channel();
        let (tx_out, rx_out) = mpsc::unbounded_channel();

        let service = Self {
            core: tx_pool_core,
            input_interface: TxPoolInputTaskReceiver { receiver: rx_in },
            output_inteface: TxPoolOutputTaskSender { sender: tx_out },
        };

        TxPoolInstantiationArtifacts {
            service,
            input_sender: TxPoolInputTaskSender { sender: tx_in },
            output_receiver: TxPoolOutputTaskReceiver { receiver: rx_out },
        }
    }

    /// Runs transaction pool service expecting to receive tasks from the
    /// tx pool input task sender.
    pub async fn run(mut self) {
        while let Some(task) = self.input_interface.recv().await {
            match task {
                InputTask::AddTransaction {
                    transaction,
                    response_sender,
                } => {
                    let res = self.core.add_transaction(transaction.clone());
                    if let Some(response_sender) = response_sender {
                        let _ = response_sender.send(res).inspect_err(|err| {
                            log::error!("`AddTransaction` task receiver dropped - {err:?}")
                        });
                    }

                    if let Err(err) = self
                        .output_inteface
                        .send(OutputTask::PropogateTransaction { transaction })
                    {
                        log::error!("Failed to send `PropogateTransaction` task: {err:?}");
                    }
                }
            }
        }
    }
}

mod input {
    use anyhow::Result;
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
            response_sender: Option<oneshot::Sender<Result<()>>>,
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
        /// Signals to the external service to propogate the transaction
        PropogateTransaction { transaction: Tx },
    }

    /// Transaction pool output task sender.
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
