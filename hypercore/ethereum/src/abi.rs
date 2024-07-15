use alloy::{
    primitives::{Address, Bytes, FixedBytes, B256},
    sol,
};
use hypercore_common::{BlockCommitment, CodeCommitment, OutgoingMessage, StateTransition};

sol!(
    #[sol(rpc)]
    IProgram,
    "Program.json"
);

sol!(
    #[sol(rpc)]
    IRouter,
    "Router.json"
);

sol!(
    #[sol(rpc)]
    IWrappedVara,
    "WrappedVara.json"
);

impl From<CodeCommitment> for IRouter::CodeCommitment {
    fn from(commitment: CodeCommitment) -> Self {
        Self {
            codeId: B256::new(commitment.code_id.into_bytes()),
            approved: commitment.approved,
        }
    }
}

impl From<OutgoingMessage> for IRouter::OutgoingMessage {
    fn from(msg: OutgoingMessage) -> Self {
        let reply_details = msg.reply_details.unwrap_or_default();
        IRouter::OutgoingMessage {
            destination: {
                let mut address = Address::ZERO;
                address
                    .0
                    .copy_from_slice(&msg.destination.into_bytes()[12..]);
                address
            },
            payload: Bytes::copy_from_slice(msg.payload.inner()),
            value: msg.value,
            replyDetails: IRouter::ReplyDetails {
                replyTo: B256::new(reply_details.to_message_id().into_bytes()),
                replyCode: FixedBytes::new(reply_details.to_reply_code().to_bytes()),
            },
        }
    }
}

impl From<StateTransition> for IRouter::StateTransition {
    fn from(transition: StateTransition) -> Self {
        Self {
            actorId: {
                let mut address = Address::ZERO;
                address
                    .0
                    .copy_from_slice(&transition.actor_id.into_bytes()[12..]);
                address
            },
            oldStateHash: B256::new(transition.old_state_hash.to_fixed_bytes()),
            newStateHash: B256::new(transition.new_state_hash.to_fixed_bytes()),
            outgoingMessages: transition
                .outgoing_messages
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<BlockCommitment> for IRouter::BlockCommitment {
    fn from(commitment: BlockCommitment) -> Self {
        Self {
            blockHash: B256::new(commitment.block_hash.to_fixed_bytes()),
            allowedPredBlockHash: B256::new(commitment.allowed_pred_block_hash.to_fixed_bytes()),
            allowedPrevCommitmentHash: B256::new(
                commitment.allowed_prev_commitment_hash.to_fixed_bytes(),
            ),
            transitions: commitment.transitions.into_iter().map(Into::into).collect(),
        }
    }
}
