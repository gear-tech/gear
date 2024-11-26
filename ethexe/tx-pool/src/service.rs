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

use crate::{Transaction, TxPoolTrait};
use input::TxPoolInputTaskReceiver;
use tokio::sync::mpsc;

/// Transaction pool service.
///
/// Serves as an interface for the transaction pool core.
pub struct TxPoolService<Tx: Transaction, TxPool: TxPoolTrait<Transaction = Tx>> {
    core: TxPool,
    input_interface: TxPoolInputTaskReceiver<Tx>,
}

impl<Tx: Transaction, TxPool: TxPoolTrait<Transaction = Tx>> TxPoolService<Tx, TxPool> {
    pub fn new(tx_pool_core: impl Into<TxPool>) -> (Self, TxPoolInputTaskSender<Tx>) {
        let tx_pool_core = tx_pool_core.into();
        let (tx, rx) = mpsc::unbounded_channel();

        let tx_pool_interface = Self {
            core: tx_pool_core,
            input_interface: TxPoolInputTaskReceiver { receiver: rx },
        };

        (tx_pool_interface, TxPoolInputTaskSender { sender: tx })
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
                    if response_sender
                        .send(self.core.add_transaction(transaction))
                        .is_err()
                    {
                        log::debug!("`AddTransaction` task receiver dropped.")
                    }
                }
            }
        }
    }
}

// TODO [sab] move I\O to the other crate so ethexe-rpc is lighter
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
        /// Sends the response back to the task sender.
        AddTransaction {
            transaction: Tx,
            response_sender: oneshot::Sender<Result<()>>,
        },
    }

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
