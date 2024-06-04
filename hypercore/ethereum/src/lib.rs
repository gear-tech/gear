#![allow(dead_code, clippy::new_without_default)]

use alloy::{
    consensus::{SidecarBuilder, SignableTransaction, SimpleCoder},
    network::{Ethereum, EthereumSigner, TxSigner},
    primitives::{Address, Bytes, ChainId, Signature, B256},
    providers::{
        fillers::{ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, SignerFiller},
        Identity, ProviderBuilder, RootProvider,
    },
    pubsub::PubSubFrontend,
    rpc::client::WsConnect,
    signers::{
        self as alloy_signer, sign_transaction_with_chain_id, Error as SignerError,
        Result as SignerResult, Signer, SignerSync,
    },
    sol,
};
use anyhow::Result;
use async_trait::async_trait;
use gear_core::ids::prelude::*;
use gprimitives::{ActorId, CodeId, MessageId, H256};
use hypercore_signer::{PublicKey, Signer as HypercoreSigner};

pub mod event;

sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    AlloyRouter,
    "router_abi.json"
);

sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    AlloyProgram,
    "program_abi.json"
);

type AlloyTransport = PubSubFrontend;
type AlloyRecommendFiller =
    JoinFill<JoinFill<JoinFill<Identity, GasFiller>, NonceFiller>, ChainIdFiller>;
type AlloyProvider = FillProvider<
    JoinFill<AlloyRecommendFiller, SignerFiller<EthereumSigner>>,
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
    #[inline]
    fn sign_hash_sync(&self, hash: &B256) -> SignerResult<Signature> {
        let signature = self
            .signer
            .sign_digest(self.sender, hash.0)
            .map_err(|err| SignerError::Other(err.into()))?;
        Ok(Signature::try_from(&signature.0[..])?)
    }

    #[inline]
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

pub struct CreateProgramData {
    pub salt: Vec<u8>,
    pub code_id: CodeId,
    pub state_hash: H256,
}

pub struct UpdateProgramData {
    pub program: ActorId,
    pub state_hash: H256,
}

pub struct CommitData {
    pub code_ids: Vec<CodeId>,
    pub create_programs: Vec<CreateProgramData>,
    pub update_programs: Vec<UpdateProgramData>,
}

pub struct Router(AlloyRouterInstance);

impl Router {
    pub async fn new(address: &str, rpc_url: &str, sender: Sender) -> Result<Self> {
        Ok(Self(AlloyRouterInstance::new(
            Address::parse_checksummed(address, None)?,
            create_provider(rpc_url, sender).await?,
        )))
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
        salt: impl AsRef<[u8]>,
        init_payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<H256> {
        let builder = self.0.createProgram(
            B256::new(code_id.into_bytes()),
            Bytes::copy_from_slice(salt.as_ref()),
            Bytes::copy_from_slice(init_payload.as_ref()),
            gas_limit,
            value,
        );
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
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

    pub async fn commit(&self, commit_data: CommitData) -> Result<H256> {
        let builder = self.0.commit(AlloyRouter::CommitData {
            codeIdsArray: commit_data
                .code_ids
                .into_iter()
                .map(|code_id| B256::new(code_id.into_bytes()))
                .collect(),
            createProgramsArray: commit_data
                .create_programs
                .into_iter()
                .map(|data| AlloyRouter::CreateProgramData {
                    salt: Bytes::copy_from_slice(&data.salt),
                    codeId: B256::new(data.code_id.into_bytes()),
                    stateHash: B256::new(data.state_hash.to_fixed_bytes()),
                })
                .collect(),
            updateProgramsArray: commit_data
                .update_programs
                .into_iter()
                .map(|data| AlloyRouter::UpdateProgramData {
                    program: {
                        let mut address = Address::ZERO;
                        address.0.copy_from_slice(&data.program.into_bytes()[12..]);
                        address
                    },
                    stateHash: B256::new(data.state_hash.to_fixed_bytes()),
                })
                .collect(),
        });
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
        let builder = self.0.sendMessage(
            {
                let mut address = Address::ZERO;
                address.0.copy_from_slice(&destination.into_bytes()[12..]);
                address
            },
            Bytes::copy_from_slice(payload.as_ref()),
            gas_limit,
            value,
        );
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
        let builder = self.0.sendReply(
            B256::new(reply_to_id.into_bytes()),
            Bytes::copy_from_slice(payload.as_ref()),
            gas_limit,
            value,
        );
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
