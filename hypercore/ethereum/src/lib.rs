#![allow(dead_code, clippy::new_without_default)]

use crate::event::CodeApproved;
use alloy::{
    consensus::{SidecarBuilder, SignableTransaction, SimpleCoder},
    network::{Ethereum as AlloyEthereum, EthereumWallet, TxSigner},
    primitives::{keccak256, Address, Bytes, ChainId, FixedBytes, Signature, B256, U256},
    providers::{
        fillers::{FillProvider, JoinFill, RecommendedFiller, WalletFiller},
        Provider, ProviderBuilder, RootProvider,
    },
    signers::{
        self as alloy_signer, sign_transaction_with_chain_id, Error as SignerError,
        Result as SignerResult, Signer, SignerSync,
    },
    transports::BoxTransport,
};
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use futures::StreamExt;
use gear_core::{
    code::{Code, CodeAndId},
    message::ReplyDetails,
};
use gear_wasm_instrument::gas_metering::Schedule;
use gprimitives::{ActorId, CodeId, MessageId, H256};
use hypercore_signer::{
    Address as HypercoreAddress, PublicKey, Signature as HypercoreSignature,
    Signer as HypercoreSigner,
};
use std::sync::Arc;

pub use abi::{IProgram, IRouter, IWrappedVara};

mod abi;
mod eip1167;
pub mod event;

type AlloyTransport = BoxTransport;
type AlloyProvider = FillProvider<
    JoinFill<RecommendedFiller, WalletFiller<EthereumWallet>>,
    RootProvider<AlloyTransport>,
    AlloyTransport,
    AlloyEthereum,
>;

type AlloyProgramInstance = IProgram::IProgramInstance<AlloyTransport, Arc<AlloyProvider>>;
type AlloyRouterInstance = IRouter::IRouterInstance<AlloyTransport, Arc<AlloyProvider>>;

type QueryRouterInstance =
    IRouter::IRouterInstance<AlloyTransport, Arc<RootProvider<BoxTransport>>>;

#[derive(Debug, Clone)]
struct Sender {
    signer: HypercoreSigner,
    sender: PublicKey,
    chain_id: Option<ChainId>,
}

impl Sender {
    pub fn new(signer: HypercoreSigner, sender_address: HypercoreAddress) -> Result<Self> {
        let sender = signer
            .get_key_by_addr(sender_address)?
            .ok_or_else(|| anyhow!("no key found for {sender_address}"))?;
        Ok(Self {
            signer,
            sender,
            chain_id: None,
        })
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

#[derive(Debug, Clone)]
pub struct CodeCommitment {
    pub code_id: CodeId,
    pub approved: bool,
}

#[derive(Debug, Clone)]
pub struct StateTransition {
    pub actor_id: ActorId,
    pub old_state_hash: H256,
    pub new_state_hash: H256,
    pub outgoing_messages: Vec<OutgoingMessage>,
}

#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub reply_details: ReplyDetails,
}

pub struct BlockCommitment {
    pub block_hash: H256,
    pub allowed_pred_block_hash: H256,
    pub allowed_prev_commitment_hash: H256,
    pub transitions: Vec<StateTransition>,
}

pub struct Router(AlloyRouterInstance);

impl Router {
    fn new(address: Address, provider: Arc<AlloyProvider>) -> Self {
        Self(AlloyRouterInstance::new(address, provider))
    }

    pub fn address(&self) -> HypercoreAddress {
        HypercoreAddress(*self.0.address().0)
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

    pub async fn upload_code_with_sidecar(&self, code: &[u8]) -> Result<(H256, CodeId)> {
        let schedule = Schedule::default();
        let code = Code::try_new(
            code.to_vec(),
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            schedule.limits.stack_height,
            schedule.limits.data_segments_amount.into(),
            schedule.limits.table_number.into(),
        )
        .map_err(|err| anyhow!("failed to validate code: {err}"))?;
        let (code, code_id) = CodeAndId::new(code).into_parts();

        let builder = self
            .0
            .uploadCode(B256::new(code_id.into_bytes()), B256::ZERO)
            .sidecar(SidecarBuilder::<SimpleCoder>::from_slice(code.original_code()).build()?);
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok((H256(receipt.transaction_hash.0), code_id))
    }

    pub async fn wait_for_code_approval(&self, code_id: CodeId) -> Result<CodeApproved> {
        let mut code_approved_filter = self.0.CodeApproved_filter();
        code_approved_filter.filter = code_approved_filter
            .filter
            .topic1(B256::new(code_id.into_bytes()));

        let code_approved_subscription = code_approved_filter.subscribe().await?;
        let mut code_approved_stream = code_approved_subscription.into_stream();

        let Some(Ok((_, ref log))) = code_approved_stream.next().await else {
            bail!("failed to read CodeApproved event");
        };

        log.try_into()
    }

    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: H256,
        init_payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(H256, ActorId)> {
        let mut buffer = vec![];
        buffer.extend_from_slice(salt.as_ref());
        buffer.extend_from_slice(code_id.as_ref());
        let create2_salt = keccak256(buffer);
        let program = self.0.program().call().await?._0;
        let proxy_bytecode = eip1167::minimal_proxy_bytecode(*program.0);
        let actor_id = ActorId::new(
            self.0
                .address()
                .create2_from_code(create2_salt, proxy_bytecode)
                .into_word()
                .0,
        );
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
        Ok((H256(receipt.transaction_hash.0), actor_id))
    }

    pub async fn commit_codes(
        &self,
        commitments: Vec<CodeCommitment>,
        signatures: Vec<HypercoreSignature>,
    ) -> Result<H256> {
        let builder = self.0.commitCodes(
            commitments
                .into_iter()
                .map(|commitment| IRouter::CodeCommitment {
                    codeId: B256::new(commitment.code_id.into_bytes()),
                    approved: commitment.approved,
                })
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

    fn convert_block_commitment(commitment: BlockCommitment) -> IRouter::BlockCommitment {
        let transitions =
            commitment
                .transitions
                .into_iter()
                .map(|transition| IRouter::StateTransition {
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
                        .map(|outgoing_message| IRouter::OutgoingMessage {
                            destination: {
                                let mut address = Address::ZERO;
                                address.0.copy_from_slice(
                                    &outgoing_message.destination.into_bytes()[12..],
                                );
                                address
                            },
                            payload: Bytes::copy_from_slice(&outgoing_message.payload),
                            value: outgoing_message.value,
                            replyDetails: IRouter::ReplyDetails {
                                replyTo: B256::new(
                                    outgoing_message.reply_details.to_message_id().into_bytes(),
                                ),
                                replyCode: FixedBytes::new(
                                    outgoing_message.reply_details.to_reply_code().to_bytes(),
                                ),
                            },
                        })
                        .collect(),
                });

        IRouter::BlockCommitment {
            blockHash: B256::new(commitment.block_hash.to_fixed_bytes()),
            allowedPredBlockHash: B256::new(commitment.allowed_pred_block_hash.to_fixed_bytes()),
            allowedPrevCommitmentHash: B256::new(
                commitment.allowed_prev_commitment_hash.to_fixed_bytes(),
            ),
            transitions: transitions.collect(),
        }
    }

    pub async fn commit_blocks(
        &self,
        commitments: Vec<BlockCommitment>,
        signatures: Vec<HypercoreSignature>,
    ) -> Result<H256> {
        let builder = self
            .0
            .commitBlocks(
                commitments
                    .into_iter()
                    .map(Self::convert_block_commitment)
                    .collect(),
                signatures
                    .into_iter()
                    .map(|signature| Bytes::copy_from_slice(&signature.0))
                    .collect(),
            )
            .gas(10_000_000);
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }
}

pub struct RouterQuery(QueryRouterInstance);

impl RouterQuery {
    pub async fn new(rpc_url: &str, router_address: HypercoreAddress) -> Result<Self> {
        let provider = Arc::new(ProviderBuilder::new().on_builtin(rpc_url).await?);
        Ok(Self(QueryRouterInstance::new(
            Address::new(router_address.0),
            provider,
        )))
    }

    pub async fn last_commitment_block_hash(&self) -> Result<H256> {
        self.0
            .lastBlockCommitmentHash()
            .call()
            .await
            .map(|res| H256(*res._0))
            .map_err(Into::into)
    }

    pub async fn genesis_block_hash(&self) -> Result<H256> {
        self.0
            .genesisBlockHash()
            .call()
            .await
            .map(|res| H256(*res._0))
            .map_err(Into::into)
    }
}

pub struct Program(AlloyProgramInstance);

impl Program {
    fn new(address: Address, provider: Arc<AlloyProvider>) -> Self {
        Self(AlloyProgramInstance::new(address, provider))
    }

    pub fn address(&self) -> HypercoreAddress {
        HypercoreAddress(*self.0.address().0)
    }

    pub async fn send_message(
        &self,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<H256> {
        let builder = self
            .0
            .sendMessage(Bytes::copy_from_slice(payload.as_ref()), gas_limit)
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

async fn create_provider(
    rpc_url: &str,
    signer: HypercoreSigner,
    sender_address: HypercoreAddress,
) -> Result<Arc<AlloyProvider>> {
    Ok(Arc::new(
        ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(EthereumWallet::new(Sender::new(signer, sender_address)?))
            .on_builtin(rpc_url)
            .await?,
    ))
}

pub struct Ethereum {
    router_address: Address,
    provider: Arc<AlloyProvider>,
}

impl Ethereum {
    pub async fn new(
        rpc_url: &str,
        router_address: HypercoreAddress,
        signer: HypercoreSigner,
        sender_address: HypercoreAddress,
    ) -> Result<Self> {
        Ok(Self {
            router_address: Address::new(router_address.0),
            provider: create_provider(rpc_url, signer, sender_address).await?,
        })
    }

    pub async fn deploy(
        rpc_url: &str,
        validators: Vec<HypercoreAddress>,
        signer: HypercoreSigner,
        sender_address: HypercoreAddress,
    ) -> Result<Self> {
        const VALUE_PER_GAS: u128 = 6;

        let provider = create_provider(rpc_url, signer, sender_address).await?;
        let validators = validators
            .into_iter()
            .map(|validator_address| Address::new(validator_address.0))
            .collect();
        let deployer_address = Address::new(sender_address.0);

        let wrapped_vara =
            IWrappedVara::deploy(provider.clone(), deployer_address, VALUE_PER_GAS).await?;
        let wrapped_vara_address = *wrapped_vara.address();

        let nonce = provider.get_transaction_count(deployer_address).await?;
        let program_address = deployer_address.create(
            nonce
                .checked_add(1)
                .ok_or_else(|| anyhow!("failed to add one"))?,
        );

        let router = IRouter::deploy(
            provider.clone(),
            deployer_address,
            program_address,
            wrapped_vara_address,
            validators,
        )
        .await?;
        let router_address = *router.address();

        IProgram::deploy(provider.clone(), router_address).await?;

        let builder = wrapped_vara.approve(router_address, U256::MAX);
        builder.send().await?.get_receipt().await?;

        Ok(Self {
            router_address,
            provider,
        })
    }

    pub fn provider(&self) -> Arc<AlloyProvider> {
        self.provider.clone()
    }

    pub fn router(&self) -> Router {
        Router::new(self.router_address, self.provider.clone())
    }

    pub fn program(&self, program_address: HypercoreAddress) -> Program {
        Program::new(Address::new(program_address.0), self.provider.clone())
    }
}
