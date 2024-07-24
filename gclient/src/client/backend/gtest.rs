// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    client::{Message, Program},
    Backend, Code, TxResult,
};
use anyhow::Result;
use async_trait::async_trait;
use gear_core::{ids::ProgramId, message::UserStoredMessage};
use gprimitives::{ActorId, MessageId};
use gsdk::metadata::runtime_types::gear_common::storage::primitives::Interval;
use gtest::System;
use parity_scale_codec::Decode;
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
    task::{JoinHandle, LocalSet},
};

/// gear general client gtest backend
pub struct Gtest {
    tx: Sender<Request>,
    results: Arc<Mutex<HashMap<usize, Response>>>,
    timeout: Duration,
    handle: JoinHandle<()>,
}

impl Gtest {
    /// New gtest instance
    pub fn new(size: usize, timeout: Duration) -> Self {
        let local = LocalSet::new();
        let results = Arc::new(Mutex::new(HashMap::new()));
        let (tx, mut rx) = mpsc::channel::<Request>(size);

        let cloned = results.clone();
        let handle = local.spawn_local(async move {
            let system = System::new();
            while let Some(tx) = rx.recv().await {
                let (result, nounce) = match tx {
                    Request::Deploy {
                        nounce,
                        code,
                        message,
                        signer,
                    } => (handle::deploy(&system, code, message, signer), nounce),
                    _ => {
                        todo!()
                    }
                };

                cloned.lock().await.insert(nounce, result);
            }
        });

        Self {
            tx,
            results,
            timeout,
            handle,
        }
    }
}

#[async_trait]
impl Backend for Gtest {
    async fn program(&self, id: ProgramId) -> Result<Program<Self>> {
        todo!()
    }

    async fn deploy<M>(&self, _code: impl Code, message: M) -> Result<TxResult<Program<Self>>>
    where
        M: Into<Message> + Send,
    {
        todo!()
    }

    async fn send<M>(&self, _id: ProgramId, message: M) -> Result<TxResult<MessageId>>
    where
        M: Into<Message> + Send,
    {
        todo!()
    }

    async fn message(&self, mid: MessageId) -> Result<Option<(UserStoredMessage, Interval<u32>)>> {
        todo!()
    }
}

/// Gtest transactions
pub enum Request {
    Deploy {
        nounce: usize,
        code: Vec<u8>,
        message: Message,
        signer: ActorId,
    },
    Send(Message),
}

/// Gtest handle result
pub enum Response {
    Deploy(TxResult<ActorId>),
    Send(TxResult<MessageId>),
}

/// gtest handles
pub(crate) mod handle {
    use crate::{client::backend::gtest::Response, Message, TxResult};
    use gear_core::{
        buffer::LimitedVec,
        ids::prelude::CodeIdExt,
        message::{ReplyDetails, UserMessage},
    };
    use gprimitives::{ActorId, CodeId};
    use gtest::{CoreLog, Program, System};

    /// Deploy program
    pub fn deploy(system: &System, code: Vec<u8>, message: Message, signer: ActorId) -> Response {
        let id = CodeId::generate(&code);
        let prog = Program::from_binary_with_id(system, code, &id.into_bytes());
        let r = prog.send_bytes(signer, message.payload);

        Response::Deploy(TxResult {
            result: prog.id(),
            logs: map_logs(r.log()),
        })
    }

    fn map_logs(logs: &[CoreLog]) -> Vec<UserMessage> {
        logs.into_iter()
            .map(|l| {
                UserMessage::new(
                    l.id(),
                    l.source(),
                    l.destination(),
                    LimitedVec::try_from(l.payload().to_vec()).unwrap_or_default(),
                    Default::default(),
                    l.reply_code()
                        .zip(l.reply_to())
                        .map(|(code, to)| ReplyDetails::new(to, code)),
                )
            })
            .collect()
    }
}
