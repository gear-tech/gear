// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    block_gas_cost,
    gear_calls::{
        ExtrinsicGeneratorSet, MailboxProvider, RepeatedGenerator, SendMessageGenerator,
        SendReplyGenerator, UploadProgramGenerator,
    },
    runtime::{self, default_gas_limit, get_mailbox_messages},
    FuzzingConfig,
};
use gear_core::ids::MessageId;
use gear_runtime::AccountId;

#[cfg(test)]
pub fn min_unstructured_input_size() -> usize {
    let generators = default_generator_set("".to_string());
    generators.unstructured_size_hint()
}

const UPLOAD_PROGRAM_CALLS: usize = 10;
const SEND_MESSAGE_CALLS: usize = 15;
const SEND_REPLY_CALLS: usize = 1;
const OVERALL_CALLS_COUNT: usize = UPLOAD_PROGRAM_CALLS + SEND_MESSAGE_CALLS + SEND_REPLY_CALLS;

const MIN_VALUE_SENT: u128 = 0;
const MAX_VALUE_SENT: u128 = 100_000;

pub(crate) fn default_generator_set(test_input_id: String) -> ExtrinsicGeneratorSet {
    ExtrinsicGeneratorSet::new(vec![
        RepeatedGenerator::new(
            UPLOAD_PROGRAM_CALLS,
            UploadProgramGenerator {
                gas: default_gas_limit(),
                value: MIN_VALUE_SENT..=MAX_VALUE_SENT,
                test_input_id,
            }
            .into(),
        ),
        RepeatedGenerator::new(
            SEND_MESSAGE_CALLS,
            SendMessageGenerator {
                gas: default_gas_limit(),
                value: MIN_VALUE_SENT..=MAX_VALUE_SENT,
                prepaid: false,
            }
            .into(),
        ),
        RepeatedGenerator::new(
            SEND_REPLY_CALLS,
            SendReplyGenerator {
                mailbox_provider: Box::from(MailboxProviderImpl {
                    account_id: runtime::account(runtime::alice()),
                }),
                gas: default_gas_limit(),
                value: MIN_VALUE_SENT..=MAX_VALUE_SENT,
                prepaid: false,
            }
            .into(),
        ),
    ])
}

pub(crate) fn default_fuzzing_config() -> FuzzingConfig {
    let sender_balance = (block_gas_cost() + MAX_VALUE_SENT) * OVERALL_CALLS_COUNT as u128;

    FuzzingConfig {
        initial_sender_balance: sender_balance..=sender_balance,
        allow_overspend: false,
    }
}

struct MailboxProviderImpl {
    account_id: AccountId,
}

impl MailboxProvider for MailboxProviderImpl {
    fn fetch_messages(&self) -> Vec<MessageId> {
        get_mailbox_messages(&self.account_id)
    }
}
