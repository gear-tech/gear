// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use anyhow::Result;
use ethexe_common::gear::{Message, ValueClaim};
use ethexe_ethereum::benchmarking::{ContractImplKind, SimulationContext};
use gprimitives::{ActorId, H256, MessageId};

fn main() -> Result<()> {
    let mut simulation_context = SimulationContext::new()?;
    let mut context = simulation_context.initialize()?;

    context.switch_to_impl(ContractImplKind::Regular)?;

    let empty_batch_gas = context.empty_batch_gas()?;
    dbg!(empty_batch_gas);

    context.switch_to_impl(ContractImplKind::WithInstrumentation)?;

    let code_commitment_gas = context.code_commitment_gas()?;
    dbg!(code_commitment_gas);

    let state_transition_zero_actor_id_gas = context.state_transition_actor_id_gas(ActorId::zero());
    dbg!(state_transition_zero_actor_id_gas);

    let state_transition_non_zero_actor_id_gas =
        context.state_transition_actor_id_gas(ActorId::new([0x01; 32]));
    dbg!(state_transition_non_zero_actor_id_gas);

    let state_transition_zero_state_hash_gas =
        context.state_transition_new_state_hash_gas(H256::zero());
    dbg!(state_transition_zero_state_hash_gas);

    let state_transition_non_zero_state_hash_gas =
        context.state_transition_new_state_hash_gas(H256([0x01; 32]));
    dbg!(state_transition_non_zero_state_hash_gas);

    let state_transition_not_exited_gas = context.state_transition_exited_gas(false);
    dbg!(state_transition_not_exited_gas);

    let state_transition_exited_gas = context.state_transition_exited_gas(true);
    dbg!(state_transition_exited_gas);

    let state_transition_zero_inheritor_gas =
        context.state_transition_inheritor_gas(ActorId::zero());
    dbg!(state_transition_zero_inheritor_gas);

    let state_transition_non_zero_inheritor_gas =
        context.state_transition_inheritor_gas(ActorId::new([0x01; 32]));
    dbg!(state_transition_non_zero_inheritor_gas);

    let state_transition_zero_value_to_receive_gas =
        context.state_transition_value_to_receive_gas(0);
    dbg!(state_transition_zero_value_to_receive_gas);

    // dynamic gas cost
    let state_transition_non_zero_value_to_receive_gas =
        context.state_transition_value_to_receive_gas(1);
    dbg!(state_transition_non_zero_value_to_receive_gas);

    let state_transition_value_to_receive_positive_sign_gas =
        context.state_transition_value_to_receive_negative_sign_gas(false);
    dbg!(state_transition_value_to_receive_positive_sign_gas);

    let state_transition_value_to_receive_negative_sign_gas =
        context.state_transition_value_to_receive_negative_sign_gas(true);
    dbg!(state_transition_value_to_receive_negative_sign_gas);

    let empty_state_transition_value_claims_gas = context.state_transition_value_claims_gas(vec![]);
    dbg!(empty_state_transition_value_claims_gas);

    let empty_state_transition_messages_gas = context.state_transition_messages_gas(vec![]);
    dbg!(empty_state_transition_messages_gas);

    let verify_actor_id_gas = context.verify_actor_id_gas()?;
    dbg!(verify_actor_id_gas);

    let retrieve_ether_positive_value_gas = context.retrieve_ether_gas(0, false)?;
    dbg!(retrieve_ether_positive_value_gas);

    let retrieve_ether_negative_value_gas = context.retrieve_ether_gas(1, true)?;
    dbg!(retrieve_ether_negative_value_gas);

    // dynamic gas cost
    let send_message_gas = context.send_message_gas(Message {
        id: MessageId::zero(),
        destination: H256::random().into(),
        payload: vec![],
        value: 0,
        reply_details: None,
        call: false,
    })?;
    dbg!(send_message_gas);

    // dynamic gas cost
    let value_claim_gas = context.value_claim_gas(ValueClaim {
        message_id: MessageId::zero(),
        destination: ActorId::zero(),
        value: 0,
    })?;
    dbg!(value_claim_gas);

    let set_inheritor_not_exited_gas = context.set_inheritor_gas(None)?;
    dbg!(set_inheritor_not_exited_gas);

    let set_inheritor_exited_gas = context.set_inheritor_gas(Some(ActorId::new([0x01; 32])))?;
    dbg!(set_inheritor_exited_gas);

    // TODO: same state hash

    let update_state_hash_uninitialized_actor_id_gas =
        context.update_state_hash_gas(context.uninitialized_actor_id(), H256::random())?;
    dbg!(update_state_hash_uninitialized_actor_id_gas);

    let update_state_hash_initialized_actor_id_gas =
        context.update_state_hash_gas(context.initialized_actor_id(), H256::random())?;
    dbg!(update_state_hash_initialized_actor_id_gas);

    let state_transition_hash_gas = context.state_transition_hash_gas()?;
    dbg!(state_transition_hash_gas);

    Ok(())
}
