use std::io::Read;

pub use gbuiltin_eth_bridge::{Request as EthBridgeRequest, Response as EthBridgeResponse};

use super::BuiltinActorError;
use crate::state::bridge::BridgeBuiltinStorage;
use gear_core::ids::ActorId;
use gprimitives::{H160, H256, U256};
use parity_scale_codec::Decode;
use sp_runtime::traits::{Hash, Keccak256};

pub const ETH_BRIDGE_ID: ActorId = ActorId::new(*b"modl/bia/eth-bridge/v-\x01\0/\0\0\0\0\0\0\0");

pub(crate) fn process_eth_bridge_dispatch(
    source: ActorId,
    mut payload: &[u8],
) -> Result<EthBridgeResponse, BuiltinActorError> {
    let request =
        EthBridgeRequest::decode(&mut payload).map_err(|_| BuiltinActorError::DecodingError)?;

    // todo [sab] charge gas properly

    match request {
        EthBridgeRequest::SendEthMessage {
            destination,
            payload,
        } => {
            let (nonce, hash) = create_bridge_call_output(source, destination, payload);

            Ok(EthBridgeResponse::EthMessageQueued { nonce, hash })
        }
    }
}

pub(crate) fn create_bridge_call_output(
    source: ActorId,
    destination: H160,
    payload: Vec<u8>,
) -> (U256, H256) {
    let nonce = BridgeBuiltinStorage::fetch_nonce();

    let mut nonce_bytes = [0; 32];
    nonce.to_little_endian(&mut nonce_bytes);

    let bytes = [
        nonce_bytes.as_ref(),
        source.into_bytes().as_ref(),
        destination.as_bytes(),
        payload.as_ref(),
    ]
    .concat();

    let hash = Keccak256::hash(&bytes);

    (nonce, hash)
}
