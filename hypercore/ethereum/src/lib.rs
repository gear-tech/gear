#![allow(dead_code, clippy::new_without_default)]

use abi::{AlloyProgram, AlloyRouter};
use alloy::{
    consensus::{SidecarBuilder, SignableTransaction, SimpleCoder},
    network::{Ethereum, EthereumSigner, TxSigner},
    primitives::{keccak256, Address, Bytes, ChainId, Signature, B256},
    providers::{
        fillers::{FillProvider, JoinFill, RecommendedFiller, SignerFiller},
        ProviderBuilder, RootProvider,
    },
    pubsub::PubSubFrontend,
    rpc::client::WsConnect,
    signers::{
        self as alloy_signer, sign_transaction_with_chain_id, Error as SignerError,
        Result as SignerResult, Signer, SignerSync,
    },
};
use anyhow::Result;
use async_trait::async_trait;
use gear_core::ids::prelude::*;
use gprimitives::{ActorId, CodeId, MessageId, H256};
use hypercore_signer::{PublicKey, Signature as HypercoreSignature, Signer as HypercoreSigner};
use std::mem;

mod abi;
pub mod event;

type AlloyTransport = PubSubFrontend;
type AlloyProvider = FillProvider<
    JoinFill<RecommendedFiller, SignerFiller<EthereumSigner>>,
    RootProvider<AlloyTransport>,
    AlloyTransport,
    Ethereum,
>;

type AlloyProgramInstance = AlloyProgram::AlloyProgramInstance<AlloyTransport, AlloyProvider>;
type AlloyRouterInstance = AlloyRouter::AlloyRouterInstance<AlloyTransport, AlloyProvider>;

#[derive(Debug, Clone)]
pub struct Sender {
    signer: HypercoreSigner,
    sender: PublicKey,
    chain_id: Option<ChainId>,
}

impl Sender {
    pub fn new(signer: HypercoreSigner, sender: PublicKey) -> Self {
        Self {
            signer,
            sender,
            chain_id: None,
        }
    }
}

#[async_trait]
impl Signer for Sender {
    async fn sign_hash(&self, hash: &B256) -> SignerResult<Signature> {
        self.sign_hash_sync(hash)
    }

    fn address(&self) -> Address {
        self.sender.to_address().0.into()
    }

    fn chain_id(&self) -> Option<ChainId> {
        self.chain_id
    }

    fn set_chain_id(&mut self, chain_id: Option<ChainId>) {
        self.chain_id = chain_id;
    }
}

#[async_trait]
impl TxSigner<Signature> for Sender {
    fn address(&self) -> Address {
        self.sender.to_address().0.into()
    }

    async fn sign_transaction(
        &self,
        tx: &mut dyn SignableTransaction<Signature>,
    ) -> SignerResult<Signature> {
        sign_transaction_with_chain_id!(self, tx, self.sign_hash_sync(&tx.signature_hash()))
    }
}

impl SignerSync for Sender {
    fn sign_hash_sync(&self, hash: &B256) -> SignerResult<Signature> {
        let signature = self
            .signer
            .raw_sign_digest(self.sender, hash.0)
            .map_err(|err| SignerError::Other(err.into()))?;
        Ok(Signature::try_from(&signature.0[..])?)
    }

    fn chain_id_sync(&self) -> Option<ChainId> {
        self.chain_id
    }
}

async fn create_provider(rpc_url: &str, sender: Sender) -> Result<AlloyProvider> {
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .signer(EthereumSigner::new(sender))
        .on_ws(WsConnect::new(rpc_url))
        .await?;
    Ok(provider)
}

#[derive(Debug, Clone)]
#[repr(packed)]
pub struct Transition {
    pub actor_id: ActorId,
    pub old_state_hash: H256,
    pub new_state_hash: H256,
}

pub trait Signable {
    fn create_message(&self) -> Vec<u8>;

    fn sign(&self, signer: Sender) -> Result<HypercoreSignature> {
        let hash = keccak256(self.create_message());
        let signature = signer.sign_message_sync(hash.as_slice())?;

        Ok(HypercoreSignature(signature.into()))
    }
}

impl Signable for Vec<CodeId> {
    fn create_message(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(self.len() * mem::size_of::<CodeId>());

        for code_id in self {
            buffer.extend_from_slice(&code_id.into_bytes());
        }

        buffer
    }
}

impl Signable for Vec<Transition> {
    fn create_message(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(
            self.len() * (mem::size_of::<Address>() + mem::size_of::<H256>() * 2),
        );

        for Transition {
            actor_id,
            old_state_hash,
            new_state_hash,
        } in self
        {
            buffer.extend_from_slice(&actor_id.into_bytes()[12..]);
            buffer.extend_from_slice(old_state_hash.as_bytes());
            buffer.extend_from_slice(new_state_hash.as_bytes());
        }

        buffer
    }
}

pub struct Router(AlloyRouterInstance);

impl Router {
    pub async fn new(address: &str, rpc_url: &str, sender: Sender) -> Result<Self> {
        Ok(Self(AlloyRouterInstance::new(
            Address::parse_checksummed(address, None)?,
            create_provider(rpc_url, sender).await?,
        )))
    }

    pub async fn set_program(&self, program: ActorId) -> Result<H256> {
        let builder = self.0.setProgram({
            let mut address = Address::ZERO;
            address.0.copy_from_slice(&program.into_bytes()[12..]);
            address
        });
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn add_validators(&self, validators: Vec<ActorId>) -> Result<H256> {
        let builder = self.0.addValidators(
            validators
                .into_iter()
                .map(|actor_id| {
                    let mut address = Address::ZERO;
                    address.0.copy_from_slice(&actor_id.into_bytes()[12..]);
                    address
                })
                .collect(),
        );
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn remove_validators(&self, validators: Vec<ActorId>) -> Result<H256> {
        let builder = self.0.removeValidators(
            validators
                .into_iter()
                .map(|actor_id| {
                    let mut address = Address::ZERO;
                    address.0.copy_from_slice(&actor_id.into_bytes()[12..]);
                    address
                })
                .collect(),
        );
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn upload_code(&self, code_id: CodeId, blob_tx: H256) -> Result<H256> {
        let builder = self.0.uploadCode(
            B256::new(code_id.into_bytes()),
            B256::new(blob_tx.to_fixed_bytes()),
        );
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn upload_code_with_sidecar(&self, code: &[u8]) -> Result<H256> {
        let builder = self
            .0
            .uploadCode(B256::new(CodeId::generate(code).into_bytes()), B256::ZERO)
            .sidecar(SidecarBuilder::<SimpleCoder>::from_slice(code).build()?);
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: H256,
        init_payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<H256> {
        let builder = self
            .0
            .createProgram(
                B256::new(code_id.into_bytes()),
                B256::new(salt.to_fixed_bytes()),
                Bytes::copy_from_slice(init_payload.as_ref()),
                gas_limit,
            )
            .value(value.try_into()?);
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn commit_codes(
        &self,
        code_ids: Vec<CodeId>,
        signatures: Vec<HypercoreSignature>,
    ) -> Result<H256> {
        let builder = self.0.commitCodes(
            code_ids
                .into_iter()
                .map(|code_id| B256::new(code_id.into_bytes()))
                .collect(),
            signatures
                .into_iter()
                .map(|signature| Bytes::copy_from_slice(&signature.0))
                .collect(),
        );
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }
}

pub struct Program(AlloyProgramInstance);

impl Program {
    pub async fn new(address: &str, rpc_url: &str, sender: Sender) -> Result<Self> {
        Ok(Self(AlloyProgramInstance::new(
            Address::parse_checksummed(address, None)?,
            create_provider(rpc_url, sender).await?,
        )))
    }

    pub async fn send_message(
        &self,
        destination: ActorId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<H256> {
        let builder = self
            .0
            .sendMessage(
                {
                    let mut address = Address::ZERO;
                    address.0.copy_from_slice(&destination.into_bytes()[12..]);
                    address
                },
                Bytes::copy_from_slice(payload.as_ref()),
                gas_limit,
            )
            .value(value.try_into()?);
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn send_reply(
        &self,
        reply_to_id: MessageId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<H256> {
        let builder = self
            .0
            .sendReply(
                B256::new(reply_to_id.into_bytes()),
                Bytes::copy_from_slice(payload.as_ref()),
                gas_limit,
            )
            .value(value.try_into()?);
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn claim_value(&self, message_id: MessageId) -> Result<H256> {
        let builder = self.0.claimValue(B256::new(message_id.into_bytes()));
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }
}
