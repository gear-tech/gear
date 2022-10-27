// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

mod iterator;
mod subscription;

pub use iterator::*;
pub use subscription::*;

use crate::{Error, Result};
use async_trait::async_trait;
use gear_core::ids::MessageId;
use gp::api::generated::api::{
    gear::Event as GearEvent,
    runtime_types::{
        gear_common::event::DispatchStatus as GenDispatchStatus,
        gear_core::{
            ids::MessageId as GenMId,
            message::{
                common::{MessageDetails, ReplyDetails},
                stored::StoredMessage as GenStoredMessage,
            },
        },
    },
    Event,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DispatchStatus {
    Success,
    Failed,
    NotExecuted,
}

impl From<GenDispatchStatus> for DispatchStatus {
    fn from(other: GenDispatchStatus) -> Self {
        match other {
            GenDispatchStatus::Success => Self::Success,
            GenDispatchStatus::Failed => Self::Failed,
            GenDispatchStatus::NotExecuted => Self::NotExecuted,
        }
    }
}

impl DispatchStatus {
    pub fn succeed(&self) -> bool {
        matches!(self, DispatchStatus::Success)
    }

    pub fn failed(&self) -> bool {
        matches!(self, DispatchStatus::Failed)
    }

    pub fn not_executed(&self) -> bool {
        matches!(self, DispatchStatus::NotExecuted)
    }
}

#[async_trait(?Send)]
pub trait EventProcessor {
    fn not_waited() -> Error;

    async fn proc<T>(&mut self, predicate: impl Fn(Event) -> Option<T>) -> Result<T>;
    async fn proc_many<T>(
        &mut self,
        predicate: impl Fn(Event) -> Option<T>,
        validate: impl Fn(Vec<T>) -> (Vec<T>, bool),
    ) -> Result<Vec<T>>;

    async fn message_processed(&mut self, message_id: MessageId) -> Result<DispatchStatus> {
        let message_id: GenMId = message_id.into();

        self.proc(|e| {
            if let Event::Gear(GearEvent::MessagesDispatched { statuses, .. }) = e {
                statuses
                    .into_iter()
                    .find(|(mid, _)| mid == &message_id)
                    .map(|(_, status)| status.into())
            } else {
                None
            }
        })
        .await
    }

    async fn message_processed_batch(
        &mut self,
        message_ids: impl IntoIterator<Item = MessageId>,
    ) -> Result<Vec<(MessageId, DispatchStatus)>> {
        let message_ids: Vec<GenMId> = message_ids.into_iter().map(Into::into).collect();

        Ok(self
            .proc_many(
                |e| {
                    if let Event::Gear(GearEvent::MessagesDispatched { statuses, .. }) = e {
                        let requested: Vec<_> = statuses
                            .into_iter()
                            .filter_map(|(mid, status)| {
                                message_ids
                                    .contains(&mid)
                                    .then(|| (mid.into(), status.into()))
                            })
                            .collect();

                        (!requested.is_empty()).then_some(requested)
                    } else {
                        None
                    }
                },
                |v| {
                    let count = v.iter().flatten().count() == message_ids.len();
                    (v, count)
                },
            )
            .await?
            .into_iter()
            .flatten()
            .collect())
    }

    async fn reply_bytes_on(
        &mut self,
        message_id: MessageId,
    ) -> Result<(MessageId, Result<Vec<u8>, String>, u128)> {
        let message_id: GenMId = message_id.into();

        self.proc(|e| {
            if let Event::Gear(GearEvent::UserMessageSent {
                message:
                    GenStoredMessage {
                        id,
                        payload,
                        value,
                        details:
                            Some(MessageDetails::Reply(ReplyDetails {
                                reply_to,
                                status_code,
                            })),
                        ..
                    },
                ..
            }) = e
            {
                reply_to.eq(&message_id).then(|| {
                    let res = status_code
                        .eq(&0)
                        .then_some(payload.0.clone())
                        .ok_or_else(|| String::from_utf8(payload.0).expect("Infallible"));

                    (id.into(), res, value)
                })
            } else {
                None
            }
        })
        .await
    }

    async fn err_or_succeed(&mut self, message_id: MessageId) -> Result<Option<String>> {
        let message_id: GenMId = message_id.into();

        self.proc(|e| match e {
            Event::Gear(GearEvent::UserMessageSent {
                message:
                    GenStoredMessage {
                        payload,
                        details:
                            Some(MessageDetails::Reply(ReplyDetails {
                                reply_to,
                                status_code,
                            })),
                        ..
                    },
                ..
            }) => {
                if reply_to == message_id && status_code != 0 {
                    Some(Some(String::from_utf8(payload.0).expect("Infallible")))
                } else {
                    None
                }
            }
            Event::Gear(GearEvent::MessagesDispatched { statuses, .. }) => match statuses
                .into_iter()
                .find(|(mid, _)| mid == &message_id)
                .map(|(_, status)| status)
            {
                Some(GenDispatchStatus::Failed) | None => None,
                _ => Some(None),
            },
            _ => None,
        })
        .await
    }

    async fn err_or_succeed_batch(
        &mut self,
        message_ids: impl IntoIterator<Item = MessageId>,
    ) -> Result<Vec<(MessageId, Option<String>)>> {
        let message_ids: Vec<GenMId> = message_ids.into_iter().map(Into::into).collect();

        Ok(self
            .proc_many(
                |e| match e {
                    Event::Gear(GearEvent::UserMessageSent {
                        message:
                            GenStoredMessage {
                                payload,
                                details:
                                    Some(MessageDetails::Reply(ReplyDetails {
                                        reply_to,
                                        status_code,
                                    })),
                                ..
                            },
                        ..
                    }) => {
                        if message_ids.contains(&reply_to) && status_code != 0 {
                            Some(vec![(
                                reply_to.into(),
                                Some(String::from_utf8(payload.0).expect("Infallible")),
                            )])
                        } else {
                            None
                        }
                    }
                    Event::Gear(GearEvent::MessagesDispatched { statuses, .. }) => {
                        let requested: Vec<_> = statuses
                            .into_iter()
                            .filter_map(|(mid, status)| {
                                if message_ids.contains(&mid)
                                    && !matches!(status, GenDispatchStatus::Failed)
                                {
                                    Some((MessageId::from(mid), None))
                                } else {
                                    None
                                }
                            })
                            .collect();

                        (!requested.is_empty()).then_some(requested)
                    }
                    _ => None,
                },
                |v| {
                    let count = v.iter().flatten().count() == message_ids.len();
                    (v, count)
                },
            )
            .await?
            .into_iter()
            .flatten()
            .collect())
    }
}
