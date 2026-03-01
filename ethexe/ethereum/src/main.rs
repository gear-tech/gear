use alloy::{
    signers::local::{MnemonicBuilder, coins_bip39::English},
    sol_types::{SolCall, SolConstructor},
};
use anyhow::{Result, anyhow, bail};
use ethexe_common::{
    Digest, HashOf, ToDigest,
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, SignatureType, StateTransition},
};
use ethexe_ethereum::abi::{
    Gear, IMirror, IMirrorWithInstrumentation, IRouter, ITransparentUpgradeableProxy, IWrappedVara,
};
use gear_core::ids::prelude::CodeIdExt as _;
use gprimitives::{ActorId, CodeId, H256};
use gsigner::secp256k1::{PrivateKey, PublicKey, Secp256k1SignerExt, Signer};
use revm::{
    Database, DatabaseRef, ExecuteCommitEvm, ExecuteEvm, InspectEvm, Inspector, MainBuilder,
    MainContext, MainnetEvm,
    bytecode::{Bytecode, OpCode},
    context::{BlockEnv, CfgEnv, Context, ContextTr, JournalTr, TxEnv},
    context_interface::result::{ExecResultAndState, ExecutionResult, Output},
    database::CacheDB,
    database_interface::EmptyDB,
    inspector::{JournalExt, inspectors::GasInspector},
    interpreter::{
        CallInputs, CallOutcome, CreateInputs, CreateOutcome, Interpreter, InterpreterTypes,
        interpreter_types::{Jumps, MemoryTr, StackTr},
    },
    primitives::{Address, B256, Bytes, Log, U256, eip4844::VERSIONED_HASH_VERSION_KZG},
};

/// Default Hardhat/Anvil mnemonic.
const MNEMONIC: &str = "test test test test test test test test test test test junk";

/// Derive a [`Signer`] (with one imported key) from the
/// standard derivation index `m/44'/60'/0'/0/{index}`.
///
/// Returns the signer together with the corresponding gsigner address.
fn derive_signer(index: u32) -> Result<(Signer, PublicKey, Address)> {
    // Derive the raw k256 key via alloy's BIP-32/BIP-39 MnemonicBuilder.
    let alloy_signer = MnemonicBuilder::<English>::default()
        .phrase(MNEMONIC)
        .index(index)
        .map_err(|e| anyhow!("bad derivation index {index}: {e}"))?
        .build()
        .map_err(|e| anyhow!("mnemonic derivation failed at index {index}: {e}"))?;

    // Extract the 32-byte secret and import it into a gsigner in-memory signer.
    let seed: [u8; 32] = alloy_signer.to_bytes().0;
    let private_key = PrivateKey::from_seed(seed)?;
    let signer = Signer::memory();
    let pubkey = signer.import(private_key)?;
    let address = pubkey.to_address();

    Ok((signer, pubkey, address.into()))
}

#[derive(Default)]
struct MyInspector {
    gas_inspector: GasInspector,
}

impl<CTX, DB, INTR: InterpreterTypes> Inspector<CTX, INTR> for MyInspector
where
    DB: Database,
    CTX: ContextTr<Db = DB>,
    CTX::Journal: JournalExt,
{
    fn initialize_interp(&mut self, interp: &mut Interpreter<INTR>, _context: &mut CTX) {
        self.gas_inspector.initialize_interp(&interp.gas);
    }

    fn step(&mut self, interp: &mut Interpreter<INTR>, context: &mut CTX) {
        let opcode = interp.bytecode.opcode();
        let name = OpCode::name_by_op(opcode);

        let gas_remaining = self.gas_inspector.gas_remaining();
        let memory_size = interp.memory.size();

        println!(
            "depth:{}, PC:{}, gas:{:#x}({}), OPCODE: {:?}({:?})  refund:{:#x}({}) Stack:{:?}, Data size:{}, Memory gas:{}",
            context.journal().depth(),
            interp.bytecode.pc(),
            gas_remaining,
            gas_remaining,
            name,
            opcode,
            interp.gas.refunded(),
            interp.gas.refunded(),
            interp.stack.data(),
            memory_size,
            interp.gas.memory().expansion_cost,
        );

        self.gas_inspector.step(&interp.gas);
    }

    fn step_end(&mut self, interp: &mut Interpreter<INTR>, _context: &mut CTX) {
        self.gas_inspector.step_end(&interp.gas);
    }

    fn call_end(&mut self, _context: &mut CTX, _inputs: &CallInputs, outcome: &mut CallOutcome) {
        self.gas_inspector.call_end(outcome)
    }

    fn create_end(
        &mut self,
        _context: &mut CTX,
        _inputs: &CreateInputs,
        outcome: &mut CreateOutcome,
    ) {
        self.gas_inspector.create_end(outcome)
    }

    fn log_full(&mut self, _interp: &mut Interpreter<INTR>, _context: &mut CTX, log: Log) {
        let gas_remaining = self.gas_inspector.gas_remaining();
        dbg!(gas_remaining);
        dbg!(log);
    }
}

pub struct SimulationContext {
    evm: MainnetEvm<Context<BlockEnv, TxEnv, CfgEnv, CacheDB<EmptyDB>>, MyInspector>,
    block_number: U256,
    block_timestamp: U256,
    deployer_address: Address,
    deployer_nonce: u64,
    validators_with_keys: Vec<(Signer, PublicKey, Address)>,
}

impl SimulationContext {
    const VALIDATOR_COUNT: u32 = 4;
    const MIRROR_DEPLOYMENT_NONCE_OFFSET: u64 = 2;

    pub fn new() -> Result<Self> {
        let block_number = U256::ZERO;
        let block_timestamp = U256::ZERO;

        let mut evm = Context::mainnet()
            .with_db(CacheDB::<EmptyDB>::default())
            .with_block(BlockEnv {
                number: block_number,
                timestamp: block_timestamp,
                ..Default::default()
            })
            .build_mainnet_with_inspector(MyInspector::default());

        let (_, _, deployer_address) = derive_signer(0)?;
        let deployer_nonce = 0;

        evm.journal_mut()
            .balance_incr(deployer_address, u128::MAX.try_into().expect("infallible"))?;

        let validators_with_keys = (1..=Self::VALIDATOR_COUNT)
            .map(derive_signer)
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            evm,
            block_number,
            block_timestamp,
            deployer_address,
            deployer_nonce,
            validators_with_keys,
        })
    }

    pub fn deploy(&mut self) -> Result<()> {
        let wrapped_vara = WrappedVara::deploy(self)?;

        let precomputed_mirror_impl = self.deployer_address.create(
            self.deployer_nonce
                .checked_add(Self::MIRROR_DEPLOYMENT_NONCE_OFFSET)
                .expect("infallible"),
        );

        let mut router = Router::deploy(self, precomputed_mirror_impl, &wrapped_vara)?;

        router.lookup_genesis_hash()?;

        for _ in 0..10 {
            router.commit_batch_simple(None, vec![], ExecutionMode::ExecuteAndCommit)?;
        }

        let id = router.request_code_validation(b"code1")?;
        router.commit_batch_simple(
            None,
            vec![CodeCommitment { id, valid: true }],
            ExecutionMode::ExecuteAndCommit,
        )?;

        let uninitialized_actor_id = router.create_program(id, H256([0x01; 32]), None)?;

        router.context.evm.journal_mut().balance_incr(
            uninitialized_actor_id.to_address_lossy().0.into(),
            u128::MAX.try_into().expect("infallible"),
        )?;

        let initialized_actor_id = router.create_program(id, H256([0x02; 32]), None)?;

        router.context.evm.journal_mut().balance_incr(
            initialized_actor_id.to_address_lossy().0.into(),
            u128::MAX.try_into().expect("infallible"),
        )?;

        router.switch_to_mirror_impl(MirrorImplKind::Regular)?;

        router.commit_batch_simple(
            Some(ChainCommitment {
                transitions: vec![StateTransition {
                    actor_id: initialized_actor_id,
                    new_state_hash: H256::random(),
                    exited: false,
                    inheritor: ActorId::zero(),
                    value_to_receive: 0,
                    value_to_receive_negative_sign: false,
                    value_claims: vec![],
                    messages: vec![],
                }],
                head_announce: unsafe { HashOf::new(H256([0x01; 32])) },
            }),
            vec![],
            ExecutionMode::ExecuteAndCommit,
        )?;

        let id = router.request_code_validation(b"code2")?;
        let code_commitment_gas = router.estimate_commit_batch_gas(
            None,
            vec![CodeCommitment { id, valid: true }],
            ExecutionMode::Execute,
        )?;
        dbg!(code_commitment_gas);

        Ok(())
    }

    #[allow(dead_code)]
    fn block_number(&self) -> U256 {
        self.block_number
    }

    fn block_timestamp(&self) -> U256 {
        self.block_timestamp
    }

    fn block_timestamp_u64(&self) -> u64 {
        self.block_timestamp().try_into().expect("infallible")
    }

    fn block_hash(&self, number: U256) -> Result<H256> {
        Ok(self
            .evm
            .ctx
            .db_ref()
            .block_hash_ref(number.try_into().expect("infallible"))?
            .0
            .into())
    }

    fn parent_block_hash(&self) -> Result<H256> {
        self.block_hash(
            self.block_number
                .checked_sub(U256::ONE)
                .expect("infallible"),
        )
    }

    fn parent_block_timestamp_u64(&self) -> u64 {
        self.block_timestamp_u64()
            .checked_sub(1)
            .expect("infallible")
    }

    fn next_block(&mut self) {
        self.evm.modify_block(|block_env| {
            let one = U256::ONE;

            self.block_number += one;
            block_env.number += one;

            self.block_timestamp += one;
            block_env.timestamp += one;
        });
    }

    #[allow(dead_code)]
    fn prev_block(&mut self) {
        self.evm.modify_block(|block_env| {
            let one = U256::ONE;

            if self.block_number > U256::ZERO {
                self.block_number -= one;
                block_env.number -= one;
            }

            if self.block_timestamp > U256::ZERO {
                self.block_timestamp -= one;
                block_env.timestamp -= one;
            }
        });
    }

    fn deployer_address(&self) -> Address {
        self.deployer_address
    }

    fn deployer_nonce(&self) -> u64 {
        self.deployer_nonce
    }

    fn increment_deployer_nonce(&mut self) {
        self.deployer_nonce += 1;
    }

    fn validators(&self) -> Vec<Address> {
        self.validators_with_keys
            .iter()
            .map(|(_, _, address)| *address)
            .collect()
    }

    fn min_signers(&self) -> u16 {
        self.max_signers()
            .checked_mul(2)
            .expect("multiplication failed")
            .div_ceil(3)
    }

    fn max_signers(&self) -> u16 {
        self.validators_with_keys
            .len()
            .try_into()
            .expect("conversion failed")
    }
}

pub struct WrappedVara {
    impl_address: Address,
    proxy_address: Address,
}

impl WrappedVara {
    pub fn deploy(context: &mut SimulationContext) -> Result<Self> {
        let wrapped_vara_impl = Self::deploy_impl(context)?;
        let wrapped_vara_proxy = Self::deploy_proxy(context, wrapped_vara_impl)?;

        Ok(Self {
            impl_address: wrapped_vara_impl,
            proxy_address: wrapped_vara_proxy,
        })
    }

    fn deploy_impl(context: &mut SimulationContext) -> Result<Address> {
        let ExecutionResult::Success {
            output: Output::Create(_, Some(wrapped_vara_impl)),
            ..
        } = context.evm.transact_commit(
            TxEnv::builder()
                .caller(context.deployer_address())
                .create()
                .data(IWrappedVara::BYTECODE.clone())
                .nonce(context.deployer_nonce())
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to deploy WrappedVara contract");
        };

        context.increment_deployer_nonce();

        Ok(wrapped_vara_impl)
    }

    fn deploy_proxy(
        context: &mut SimulationContext,
        wrapped_vara_impl: Address,
    ) -> Result<Address> {
        let ExecutionResult::Success {
            output: Output::Create(_, Some(wrapped_vara_proxy)),
            ..
        } = context.evm.transact_commit(
            TxEnv::builder()
                .caller(context.deployer_address())
                .create()
                .data(
                    [
                        &ITransparentUpgradeableProxy::BYTECODE[..],
                        &SolConstructor::abi_encode(
                            &ITransparentUpgradeableProxy::constructorCall {
                                _logic: wrapped_vara_impl,
                                initialOwner: context.deployer_address(),
                                _data: Bytes::new(),
                            },
                        )[..],
                    ]
                    .concat()
                    .into(),
                )
                .nonce(context.deployer_nonce())
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to deploy TransparentUpgradeableProxy contract (WrappedVara proxy)");
        };

        context.increment_deployer_nonce();

        Ok(wrapped_vara_proxy)
    }

    pub fn impl_address(&self) -> Address {
        self.impl_address
    }

    pub fn proxy_address(&self) -> Address {
        self.proxy_address
    }
}

#[derive(Debug)]
enum ExecutionMode {
    Execute,
    ExecuteAndCommit,
    ExecuteAndInspect,
}

#[derive(Debug)]
pub enum MirrorImplKind {
    Regular,
    WithInstrumentation,
}

pub struct MirrorImpl {
    address: Address,
    mirror_impl_bytecode: Bytes,
    mirror_impl_with_instrumentation_bytecode: Bytes,
}

impl MirrorImpl {
    pub fn deploy(context: &mut SimulationContext, router_proxy: Address) -> Result<Self> {
        let (_, mirror_impl_bytecode) = Self::deploy_with_execution_mode(
            context,
            router_proxy,
            &IMirror::BYTECODE[..],
            ExecutionMode::Execute,
        )?;
        let (_, mirror_impl_with_instrumentation_bytecode) = Self::deploy_with_execution_mode(
            context,
            router_proxy,
            &IMirrorWithInstrumentation::BYTECODE[..],
            ExecutionMode::Execute,
        )?;

        let (mirror_impl, _) = Self::deploy_with_execution_mode(
            context,
            router_proxy,
            &IMirror::BYTECODE[..],
            ExecutionMode::ExecuteAndCommit,
        )?;

        Ok(Self {
            address: mirror_impl,
            mirror_impl_bytecode,
            mirror_impl_with_instrumentation_bytecode,
        })
    }

    fn deploy_with_execution_mode(
        context: &mut SimulationContext,
        router_proxy: Address,
        bytecode: &[u8],
        execution_mode: ExecutionMode,
    ) -> Result<(Address, Bytes)> {
        let tx = TxEnv::builder()
            .caller(context.deployer_address())
            .create()
            .data(
                [
                    bytecode,
                    &SolConstructor::abi_encode(&IMirror::constructorCall {
                        _router: router_proxy,
                    })[..],
                ]
                .concat()
                .into(),
            )
            .nonce(context.deployer_nonce())
            .build()
            .map_err(|_| anyhow!("failed to build TxEnv"))?;

        let execution_result = match execution_mode {
            ExecutionMode::Execute => context.evm.transact(tx)?.result,
            ExecutionMode::ExecuteAndCommit => context.evm.transact_commit(tx)?,
            ExecutionMode::ExecuteAndInspect => context.evm.inspect_one_tx(tx)?,
        };

        let ExecutionResult::Success {
            output: Output::Create(mirror_impl_bytecode, Some(mirror_impl)),
            ..
        } = execution_result
        else {
            bail!("failed to deploy Mirror contract");
        };

        if let ExecutionMode::ExecuteAndCommit = execution_mode {
            context.increment_deployer_nonce();
        }

        Ok((mirror_impl, mirror_impl_bytecode))
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub fn mirror_impl_bytecode(&self) -> &Bytes {
        &self.mirror_impl_bytecode
    }

    pub fn mirror_impl_with_instrumentation_bytecode(&self) -> &Bytes {
        &self.mirror_impl_with_instrumentation_bytecode
    }
}

pub struct Router<'a> {
    context: &'a mut SimulationContext,
    impl_address: Address,
    proxy_address: Address,
    mirror_impl: MirrorImpl,
}

impl<'a> Router<'a> {
    pub fn deploy(
        context: &'a mut SimulationContext,
        precomputed_mirror_impl: Address,
        wrapped_vara: &WrappedVara,
    ) -> Result<Self> {
        let router_impl = Self::deploy_impl(context)?;

        context.next_block();

        let middleware_address = Address::ZERO;
        let aggregated_public_key = Gear::AggregatedPublicKey {
            x: "0x1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f".parse()?,
            y: "0x70beaf8f588b541507fed6a642c5ab42dfdf8120a7f639de5122d47a69a8e8d1".parse()?,
        };

        let router_proxy = Self::deploy_proxy(
            context,
            router_impl,
            precomputed_mirror_impl,
            wrapped_vara,
            middleware_address,
            aggregated_public_key,
            context.validators(),
        )?;

        context
            .evm
            .journal_mut()
            .balance_incr(router_proxy, u128::MAX.try_into().expect("infallible"))?;

        let mirror_impl = MirrorImpl::deploy(context, router_proxy)?;
        assert_eq!(mirror_impl.address(), precomputed_mirror_impl);

        Ok(Self {
            context,
            impl_address: router_impl,
            proxy_address: router_proxy,
            mirror_impl,
        })
    }

    fn deploy_impl(context: &mut SimulationContext) -> Result<Address> {
        let ExecutionResult::Success {
            output: Output::Create(_, Some(router_impl)),
            ..
        } = context.evm.transact_commit(
            TxEnv::builder()
                .caller(context.deployer_address())
                .create()
                .data(IRouter::BYTECODE.clone())
                .nonce(context.deployer_nonce())
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to deploy Router contract");
        };

        context.increment_deployer_nonce();

        Ok(router_impl)
    }

    fn deploy_proxy(
        context: &mut SimulationContext,
        router_impl: Address,
        mirror_impl: Address,
        wrapped_vara: &WrappedVara,
        middleware_address: Address,
        aggregated_public_key: Gear::AggregatedPublicKey,
        validators: Vec<Address>,
    ) -> Result<Address> {
        let ExecutionResult::Success {
            output: Output::Create(_, Some(router_proxy)),
            ..
        } = context.evm.transact_commit(
            TxEnv::builder()
                .caller(context.deployer_address())
                .create()
                .data(
                    [
                        &ITransparentUpgradeableProxy::BYTECODE[..],
                        &SolConstructor::abi_encode(
                            &ITransparentUpgradeableProxy::constructorCall {
                                _logic: router_impl,
                                initialOwner: context.deployer_address(),
                                _data: Bytes::copy_from_slice(
                                    &IRouter::initializeCall {
                                        _owner: context.deployer_address(),
                                        _mirror: mirror_impl,
                                        _wrappedVara: wrapped_vara.proxy_address(),
                                        _middleware: middleware_address,
                                        _eraDuration: U256::from(24 * 60 * 60),
                                        _electionDuration: U256::from(2 * 60 * 60),
                                        _validationDelay: U256::from(5 * 60),
                                        _aggregatedPublicKey: aggregated_public_key,
                                        _verifiableSecretSharingCommitment: Bytes::new(),
                                        _validators: validators,
                                    }
                                    .abi_encode(),
                                ),
                            },
                        )[..],
                    ]
                    .concat()
                    .into(),
                )
                .nonce(context.deployer_nonce())
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to deploy TransparentUpgradeableProxy contract (Router proxy)");
        };

        context.increment_deployer_nonce();

        Ok(router_proxy)
    }

    pub fn impl_address(&self) -> Address {
        self.impl_address
    }

    pub fn proxy_address(&self) -> Address {
        self.proxy_address
    }

    pub fn mirror_impl(&self) -> &MirrorImpl {
        &self.mirror_impl
    }

    pub fn switch_to_mirror_impl(&mut self, kind: MirrorImplKind) -> Result<()> {
        let mirror_impl = self.mirror_impl();
        let address = mirror_impl.address();
        let code = Bytecode::new_legacy(
            match kind {
                MirrorImplKind::Regular => mirror_impl.mirror_impl_bytecode(),
                MirrorImplKind::WithInstrumentation => {
                    mirror_impl.mirror_impl_with_instrumentation_bytecode()
                }
            }
            .clone(),
        );

        let journal = self.context.evm.journal_mut();
        journal.load_account(address)?;
        journal.set_code(address, code);
        let state = journal.finalize();
        self.context.evm.commit(state);

        Ok(())
    }

    fn latest_committed_batch_hash(&mut self) -> Result<Digest> {
        let ExecResultAndState {
            result:
                ExecutionResult::Success {
                    output: Output::Call(hash),
                    ..
                },
            ..
        } = self.context.evm.transact(
            TxEnv::builder()
                .caller(self.context.deployer_address())
                .call(self.proxy_address())
                .data(IRouter::latestCommittedBatchHashCall {}.abi_encode().into())
                .nonce(self.context.deployer_nonce())
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to get latest committed batch hash");
        };

        Ok(Digest(H256::from_slice(&hash).to_fixed_bytes()))
    }

    pub fn lookup_genesis_hash(&mut self) -> Result<()> {
        self.context.next_block();

        let ExecutionResult::Success { .. } = self.context.evm.transact_commit(
            TxEnv::builder()
                .caller(self.context.deployer_address())
                .call(self.proxy_address())
                .data(IRouter::lookupGenesisHashCall {}.abi_encode().into())
                .nonce(self.context.deployer_nonce())
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to lookup genesis hash");
        };

        self.context.increment_deployer_nonce();

        Ok(())
    }

    fn request_code_validation(&mut self, code: &[u8]) -> Result<CodeId> {
        let code_id = CodeId::generate(code);

        let ExecutionResult::Success { .. } = self.context.evm.transact_commit(
            TxEnv::builder()
                .caller(self.context.deployer_address())
                .call(self.proxy_address())
                .data(
                    IRouter::requestCodeValidationCall {
                        _codeId: code_id.into_bytes().into(),
                    }
                    .abi_encode()
                    .into(),
                )
                .nonce(self.context.deployer_nonce())
                .blob_hashes(vec![B256::from([VERSIONED_HASH_VERSION_KZG; 32])])
                .max_fee_per_blob_gas(1)
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to request code validation");
        };

        self.context.increment_deployer_nonce();

        Ok(code_id)
    }

    fn create_program(
        &mut self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
    ) -> Result<ActorId> {
        let ExecutionResult::Success {
            output: Output::Call(actor_id),
            ..
        } = self.context.evm.transact_commit(
            TxEnv::builder()
                .caller(self.context.deployer_address())
                .call(self.proxy_address())
                .data(
                    IRouter::createProgramCall {
                        _codeId: code_id.into_bytes().into(),
                        _salt: salt.0.into(),
                        _overrideInitializer: override_initializer
                            .map(|initializer| initializer.to_address_lossy().0.into())
                            .unwrap_or_default(),
                    }
                    .abi_encode()
                    .into(),
                )
                .nonce(self.context.deployer_nonce())
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to create program");
        };

        self.context.increment_deployer_nonce();

        Ok(actor_id.as_ref().try_into().expect("infallible"))
    }

    fn commit_batch_tx(&mut self, batch: BatchCommitment) -> Result<TxEnv> {
        let batch_digest = batch.to_digest();

        let signatures = self
            .context
            .validators_with_keys
            .iter()
            .map(|(signer, pubkey, _)| {
                Bytes::from(
                    signer
                        .sign_for_contract_digest(
                            self.proxy_address().into(),
                            *pubkey,
                            batch_digest,
                            None,
                        )
                        .expect("infallible")
                        .into_pre_eip155_bytes(),
                )
            })
            .take(self.context.min_signers() as _)
            .collect::<Vec<_>>();

        let tx = TxEnv::builder()
            .caller(self.context.deployer_address())
            .call(self.proxy_address())
            .data(
                IRouter::commitBatchCall {
                    _batch: batch.into(),
                    _signatureType: SignatureType::ECDSA as u8,
                    _signatures: signatures,
                }
                .abi_encode()
                .into(),
            )
            .nonce(self.context.deployer_nonce())
            .build()
            .map_err(|_| anyhow!("failed to build TxEnv"))?;

        Ok(tx)
    }

    fn commit_batch(
        &mut self,
        batch: BatchCommitment,
        execution_mode: ExecutionMode,
    ) -> Result<u64> {
        let tx = self.commit_batch_tx(batch)?;

        let execution_result = match execution_mode {
            ExecutionMode::Execute => self.context.evm.transact(tx)?.result,
            ExecutionMode::ExecuteAndCommit => self.context.evm.transact_commit(tx)?,
            ExecutionMode::ExecuteAndInspect => self.context.evm.inspect_one_tx(tx)?,
        };
        let ExecutionResult::Success { gas_used, .. } = execution_result else {
            bail!("failed to commit batch");
        };

        if let ExecutionMode::ExecuteAndCommit = execution_mode {
            self.context.increment_deployer_nonce();
        }

        Ok(gas_used)
    }

    fn commit_batch_simple(
        &mut self,
        chain_commitment: Option<ChainCommitment>,
        code_commitments: Vec<CodeCommitment>,
        execution_mode: ExecutionMode,
    ) -> Result<()> {
        self.context.next_block();

        let latest_committed_batch_hash = self.latest_committed_batch_hash()?;

        self.commit_batch(
            BatchCommitment {
                block_hash: self.context.parent_block_hash()?,
                timestamp: self.context.parent_block_timestamp_u64(),
                previous_batch: latest_committed_batch_hash,
                expiry: 1,
                chain_commitment,
                code_commitments,
                validators_commitment: None,
                rewards_commitment: None,
            },
            execution_mode,
        )?;

        Ok(())
    }

    fn estimate_commit_batch_gas(
        &mut self,
        chain_commitment: Option<ChainCommitment>,
        code_commitments: Vec<CodeCommitment>,
        execution_mode: ExecutionMode,
    ) -> Result<u64> {
        let expiry = 3;

        for _ in 0..expiry {
            self.context.next_block();
        }

        let latest_committed_batch_hash = self.latest_committed_batch_hash()?;

        let gas_used = self.commit_batch(
            BatchCommitment {
                block_hash: self.context.block_hash(
                    self.context
                        .block_number()
                        .checked_sub(U256::from(3))
                        .expect("infallible"),
                )?,
                timestamp: self
                    .context
                    .block_timestamp_u64()
                    .checked_sub(3)
                    .expect("infallible"),
                previous_batch: latest_committed_batch_hash,
                expiry,
                chain_commitment,
                code_commitments,
                validators_commitment: None,
                rewards_commitment: None,
            },
            execution_mode,
        )?;

        for _ in 0..expiry {
            self.context.prev_block();
        }

        Ok(gas_used)
    }
}

fn main() -> Result<()> {
    let mut context = SimulationContext::new()?;
    context.deploy()?;
    Ok(())
}
