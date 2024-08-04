#![allow(dead_code, clippy::new_without_default)]

use abi::{
    IMinimalProgram, IProgram,
    IRouter::{self, initializeCall as RouterInitializeCall},
    ITransparentUpgradeableProxy,
    IWrappedVara::{self, initializeCall as WrappedVaraInitializeCall},
};
use alloy::{
    consensus::{SidecarBuilder, SignableTransaction, SimpleCoder},
    network::{Ethereum as AlloyEthereum, EthereumWallet, Network, TransactionBuilder, TxSigner},
    primitives::{keccak256, Address, Bytes, ChainId, Signature, B256, U256},
    providers::{
        fillers::{
            ChainIdFiller, FillProvider, FillerControlFlow, GasFiller, JoinFill, TxFiller,
            WalletFiller,
        },
        Identity, Provider, ProviderBuilder, RootProvider, SendableTx,
    },
    signers::{
        self as alloy_signer, sign_transaction_with_chain_id, Error as SignerError,
        Result as SignerResult, Signer, SignerSync,
    },
    sol_types::SolCall,
    transports::{BoxTransport, Transport, TransportResult},
};
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use ethexe_common::{events::CodeApproved, BlockCommitment, CodeCommitment};
use ethexe_signer::{
    Address as LocalAddress, PublicKey, Signature as LocalSignature, Signer as LocalSigner,
};
use futures::StreamExt;
use gear_core::code::{Code, CodeAndId};
use gear_wasm_instrument::gas_metering::Schedule;
use gprimitives::{ActorId, CodeId, MessageId, H256};
use std::sync::Arc;

mod abi;
mod eip1167;
pub mod event;

type AlloyTransport = BoxTransport;
type ExeFiller = JoinFill<
    JoinFill<JoinFill<JoinFill<Identity, GasFiller>, NonceFiller>, ChainIdFiller>,
    WalletFiller<EthereumWallet>,
>;
type AlloyProvider =
    FillProvider<ExeFiller, RootProvider<AlloyTransport>, AlloyTransport, AlloyEthereum>;

type AlloyProgramInstance = IProgram::IProgramInstance<AlloyTransport, Arc<AlloyProvider>>;
type AlloyRouterInstance = IRouter::IRouterInstance<AlloyTransport, Arc<AlloyProvider>>;

type QueryRouterInstance =
    IRouter::IRouterInstance<AlloyTransport, Arc<RootProvider<BoxTransport>>>;

#[derive(Debug, Clone)]
pub struct NonceFiller;

impl<N: Network> TxFiller<N> for NonceFiller {
    type Fillable = u64;

    fn status(&self, tx: &<N as Network>::TransactionRequest) -> FillerControlFlow {
        if tx.nonce().is_some() {
            return FillerControlFlow::Finished;
        }
        if tx.from().is_none() {
            return FillerControlFlow::missing("NonceManager", vec!["from"]);
        }
        FillerControlFlow::Ready
    }

    fn fill_sync(&self, _tx: &mut SendableTx<N>) {}

    async fn prepare<P, T>(
        &self,
        provider: &P,
        tx: &N::TransactionRequest,
    ) -> TransportResult<Self::Fillable>
    where
        P: Provider<T, N>,
        T: Transport + Clone,
    {
        let from = tx.from().expect("checked by 'ready()'");
        provider.get_transaction_count(from).await
    }

    async fn fill(
        &self,
        nonce: Self::Fillable,
        mut tx: SendableTx<N>,
    ) -> TransportResult<SendableTx<N>> {
        if let Some(builder) = tx.as_mut_builder() {
            builder.set_nonce(nonce);
        }
        Ok(tx)
    }
}

#[derive(Debug, Clone)]
struct Sender {
    signer: LocalSigner,
    sender: PublicKey,
    chain_id: Option<ChainId>,
}

impl Sender {
    pub fn new(signer: LocalSigner, sender_address: LocalAddress) -> Result<Self> {
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
        Ok(Signature::try_from(signature.as_ref())?)
    }

    fn chain_id_sync(&self) -> Option<ChainId> {
        self.chain_id
    }
}

pub struct Router(AlloyRouterInstance);

impl Router {
    fn new(address: Address, provider: Arc<AlloyProvider>) -> Self {
        Self(AlloyRouterInstance::new(address, provider))
    }

    pub fn address(&self) -> LocalAddress {
        LocalAddress(*self.0.address().0)
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

        event::decode_log::<IRouter::CodeApproved>(log).map(Into::into)
    }

    // TODO: returned program id is incorrect
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
        signatures: Vec<LocalSignature>,
    ) -> Result<H256> {
        let builder = self.0.commitCodes(
            commitments.into_iter().map(Into::into).collect(),
            signatures
                .into_iter()
                .map(|signature| Bytes::copy_from_slice(signature.as_ref()))
                .collect(),
        );
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn commit_blocks(
        &self,
        commitments: Vec<BlockCommitment>,
        signatures: Vec<LocalSignature>,
    ) -> Result<H256> {
        let builder = self
            .0
            .commitBlocks(
                commitments.into_iter().map(Into::into).collect(),
                signatures
                    .into_iter()
                    .map(|signature| Bytes::copy_from_slice(signature.as_ref()))
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
    pub async fn new(rpc_url: &str, router_address: LocalAddress) -> Result<Self> {
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

    pub fn address(&self) -> LocalAddress {
        LocalAddress(*self.0.address().0)
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
    signer: LocalSigner,
    sender_address: LocalAddress,
) -> Result<Arc<AlloyProvider>> {
    Ok(Arc::new(
        ProviderBuilder::new()
            .filler(GasFiller)
            .filler(NonceFiller)
            .filler(ChainIdFiller::default())
            .wallet(EthereumWallet::new(Sender::new(signer, sender_address)?))
            .on_builtin(rpc_url)
            .await?,
    ))
}

#[derive(Clone)]
pub struct Ethereum {
    router_address: Address,
    provider: Arc<AlloyProvider>,
}

impl Ethereum {
    pub async fn new(
        rpc_url: &str,
        router_address: LocalAddress,
        signer: LocalSigner,
        sender_address: LocalAddress,
    ) -> Result<Self> {
        Ok(Self {
            router_address: Address::new(router_address.0),
            provider: create_provider(rpc_url, signer, sender_address).await?,
        })
    }

    pub async fn deploy(
        rpc_url: &str,
        validators: Vec<LocalAddress>,
        signer: LocalSigner,
        sender_address: LocalAddress,
    ) -> Result<Self> {
        const VALUE_PER_GAS: u128 = 6;

        let provider = create_provider(rpc_url, signer, sender_address).await?;
        let validators = validators
            .into_iter()
            .map(|validator_address| Address::new(validator_address.0))
            .collect();
        let deployer_address = Address::new(sender_address.0);

        let wrapped_vara_impl = IWrappedVara::deploy(provider.clone()).await?;
        let proxy = ITransparentUpgradeableProxy::deploy(
            provider.clone(),
            *wrapped_vara_impl.address(),
            deployer_address,
            Bytes::copy_from_slice(
                &WrappedVaraInitializeCall {
                    initialOwner: deployer_address,
                    _valuePerGas: VALUE_PER_GAS,
                }
                .abi_encode(),
            ),
        )
        .await?;
        let wrapped_vara = IWrappedVara::new(*proxy.address(), provider.clone());
        let wrapped_vara_address = *wrapped_vara.address();

        let nonce = provider.get_transaction_count(deployer_address).await?;
        let program_address = deployer_address.create(
            nonce
                .checked_add(2)
                .ok_or_else(|| anyhow!("failed to add 2"))?,
        );
        let minimal_program_address = deployer_address.create(
            nonce
                .checked_add(3)
                .ok_or_else(|| anyhow!("failed to add 3"))?,
        );

        let router_impl = IRouter::deploy(provider.clone()).await?;
        let proxy = ITransparentUpgradeableProxy::deploy(
            provider.clone(),
            *router_impl.address(),
            deployer_address,
            Bytes::copy_from_slice(
                &RouterInitializeCall {
                    initialOwner: deployer_address,
                    _program: program_address,
                    _minimalProgram: minimal_program_address,
                    _wrappedVara: wrapped_vara_address,
                    validatorsArray: validators,
                }
                .abi_encode(),
            ),
        )
        .await?;
        let router_address = *proxy.address();
        let router = IRouter::new(router_address, provider.clone());

        let program = IProgram::deploy(provider.clone()).await?;
        let minimal_program = IMinimalProgram::deploy(provider.clone(), router_address).await?;

        let builder = wrapped_vara.approve(router_address, U256::MAX);
        builder.send().await?.get_receipt().await?;

        assert_eq!(router.program().call().await?._0, *program.address());
        assert_eq!(
            router.minimalProgram().call().await?._0,
            *minimal_program.address()
        );

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

    pub fn program(&self, program_address: LocalAddress) -> Program {
        Program::new(Address::new(program_address.0), self.provider.clone())
    }
}
