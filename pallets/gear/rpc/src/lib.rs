//! RPC interface for the gear module.

use codec::{Codec, Decode};
pub use gear_rpc_runtime_api::GearApi as GearRuntimeApi;
use jsonrpc_core::{Error as RpcError, ErrorCode, Result};
use jsonrpc_derive::rpc;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_rpc::number::NumberOrHex;
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, MaybeDisplay},
};
use std::convert::TryInto;
use std::sync::Arc;
#[rpc]
pub trait GearApi<BlockHash> {
    #[rpc(name = "gear_getGasAmount")]
    fn get_gas_amount(&self, at: Option<BlockHash>) -> Result<NumberOrHex>;
}

/// A struct that implements the [`GearApi`].
pub struct Gear<C, M> {
    // If you have more generics, no need to Gear<C, M, N, P, ...>
    // just use a tuple like Gear<C, (M, N, P, ...)>
    client: Arc<C>,
    _marker: std::marker::PhantomData<M>,
}

impl<C, M> Gear<C, M> {
    /// Create new `Gear` instance with the given reference to the client.
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: Default::default(),
        }
    }
}

/// Error type of this RPC api.
pub enum Error {
    /// The transaction was not decodable.
    DecodeError,
    /// The call to runtime failed.
    RuntimeError,
}

impl From<Error> for i64 {
    fn from(e: Error) -> i64 {
        match e {
            Error::RuntimeError => 1,
            Error::DecodeError => 2,
        }
    }
}

impl<C, Block> GearApi<<Block as BlockT>::Hash> for Gear<C, Block>
where
    Block: BlockT,
    C: 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
    C::Api: GearRuntimeApi<Block>,
{
    fn get_gas_amount(&self, at: Option<<Block as BlockT>::Hash>) -> Result<NumberOrHex> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(||
			// If the block hash is not supplied assume the best block.
			self.client.info().best_hash));

        let runtime_api_result = api.get_gas_amount(&at).map_err(|e| RpcError {
            code: ErrorCode::ServerError(Error::RuntimeError.into()),
            message: "Unable to query fee details.".into(),
            data: Some(format!("{:?}", e).into()),
        })?;

        let try_into_rpc_gas_amount = |value: u32| {
            value.try_into().map_err(|_| RpcError {
                code: ErrorCode::InvalidParams,
                message: format!("{} doesn't fit in NumberOrHex representation", value),
                data: None,
            })
        };

        Ok(try_into_rpc_gas_amount(runtime_api_result)?)
    }
}
