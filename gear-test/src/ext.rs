use core_processor::{
    configs::{AllocationsConfig, BlockInfo},
    ProcessorExt,
};
use gear_backend_common::{ExtInfo, IntoExtInfo};
use gear_core::{
    costs::{HostFnWeights, RuntimeCosts},
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAllowanceCounter, GasAmount, GasCounter, ValueCounter},
    ids::{CodeId, MessageId, ProgramId},
    memory::{AllocationsContext, Memory, PageBuf, PageNumber, WasmPageNumber},
    message::{HandlePacket, InitPacket, MessageContext, ReplyPacket},
};
use gear_core_errors::{ExtError, MemoryError};
use std::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};

/// Structure providing externalities for running host functions.
pub struct Ext {
    /// Gas counter.
    pub gas_counter: GasCounter,
    /// Gas allowance counter.
    pub gas_allowance_counter: GasAllowanceCounter,
    /// Value counter.
    pub value_counter: ValueCounter,
    /// Allocations context.
    pub allocations_context: AllocationsContext,
    /// Message context.
    pub message_context: MessageContext,
    /// Block info.
    pub block_info: BlockInfo,
    /// Allocations config.
    pub config: AllocationsConfig,
    /// Account existential deposit
    pub existential_deposit: u128,
    /// Any guest code panic explanation, if available.
    pub error_explanation: Option<ExtError>,
    /// Contains argument to the `exit` if it was called.
    pub exit_argument: Option<ProgramId>,
    /// Communication origin
    pub origin: ProgramId,
    /// Current program id
    pub program_id: ProgramId,
    /// Map of code hashes to program ids of future programs, which are planned to be
    /// initialized with the corresponding code (with the same code hash).
    pub program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
    /// Weights of host functions.
    pub host_fn_weights: HostFnWeights,
}

/// Empty implementation for non-substrate (and non-lazy-pages) using
impl ProcessorExt for Ext {
    type Error = ExtError;

    fn new(
        gas_counter: GasCounter,
        gas_allowance_counter: GasAllowanceCounter,
        value_counter: ValueCounter,
        allocations_context: AllocationsContext,
        message_context: MessageContext,
        block_info: BlockInfo,
        config: AllocationsConfig,
        existential_deposit: u128,
        exit_argument: Option<ProgramId>,
        origin: ProgramId,
        program_id: ProgramId,
        program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
        host_fn_weights: HostFnWeights,
    ) -> Self {
        Self {
            gas_counter,
            gas_allowance_counter,
            value_counter,
            allocations_context,
            message_context,
            block_info,
            config,
            existential_deposit,
            error_explanation: None,
            exit_argument,
            origin,
            program_id,
            program_candidates_data,
            host_fn_weights,
        }
    }

    fn is_lazy_pages_enabled() -> bool {
        false
    }

    fn check_lazy_pages_consistent_state() -> bool {
        true
    }

    fn lazy_pages_protect_and_init_info(
        _mem: &dyn Memory,
        _memory_pages: &BTreeSet<PageNumber>,
        _prog_id: ProgramId,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }

    fn lazy_pages_post_execution_actions(
        _mem: &dyn Memory,
        _memory_pages: &mut BTreeMap<PageNumber, PageBuf>,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }
}

impl IntoExtInfo for Ext {
    fn into_ext_info(self, memory: &dyn Memory) -> Result<ExtInfo, (MemoryError, GasAmount)> {
        let wasm_pages = self.allocations_context.allocations().clone();
        let mut pages_data = BTreeMap::new();
        for page in wasm_pages.iter().flat_map(|p| p.to_gear_pages_iter()) {
            let mut buf = PageBuf::new_zeroed();
            if let Err(err) = memory.read(page.offset(), buf.as_mut_slice()) {
                return Err((err, self.gas_counter.into()));
            }
            pages_data.insert(page, buf);
        }

        let (outcome, context_store) = self.message_context.drain();
        let (generated_dispatches, awakening) = outcome.drain();

        Ok(ExtInfo {
            gas_amount: self.gas_counter.into(),
            allocations: wasm_pages,
            pages_data,
            generated_dispatches,
            awakening,
            context_store,
            trap_explanation: self.error_explanation,
            exit_argument: self.exit_argument,
            program_candidates_data: self.program_candidates_data,
        })
    }

    fn into_gas_amount(self) -> GasAmount {
        self.gas_counter.into()
    }
}

impl Ext {
    /// Return result and store error info in field
    pub fn return_and_store_err<T>(&mut self, result: Result<T, ExtError>) -> Result<T, ExtError> {
        result.map_err(|err| {
            self.error_explanation = Some(err);
            err
        })
    }
}

impl EnvExt for Ext {
    type Error = ExtError;

    fn alloc(
        &mut self,
        pages_num: WasmPageNumber,
        mem: &mut dyn Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
        // Greedily charge gas for allocations
        self.charge_gas(pages_num.0.saturating_mul(self.config.alloc_cost as u32))?;
        // Greedily charge gas for grow
        self.charge_gas(pages_num.0.saturating_mul(self.config.mem_grow_cost as u32))?;

        self.charge_gas_runtime(RuntimeCosts::Alloc)?;

        let old_mem_size = mem.size();

        let result = self
            .allocations_context
            .alloc(pages_num, mem)
            .map_err(ExtError::Alloc);

        let page_number = self.return_and_store_err(result)?;

        // Returns back greedily used gas for grow
        let new_mem_size = mem.size();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back = self
            .config
            .mem_grow_cost
            .saturating_mul((pages_num - grow_pages_num).0 as u64);

        // Returns back greedily used gas for allocations
        let first_page = page_number;
        let last_page = first_page + pages_num - 1.into();
        let mut new_alloced_pages_num = 0;
        for page in first_page.0..=last_page.0 {
            if !self.allocations_context.is_init_page(page.into()) {
                new_alloced_pages_num += 1;
            }
        }
        gas_to_return_back = gas_to_return_back.saturating_add(
            self.config
                .alloc_cost
                .saturating_mul((pages_num.0 - new_alloced_pages_num) as u64),
        );

        self.refund_gas(gas_to_return_back as u32)?;

        Ok(page_number)
    }

    fn block_height(&mut self) -> Result<u32, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::BlockHeight)?;
        Ok(self.block_info.height)
    }

    fn block_timestamp(&mut self) -> Result<u64, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::BlockTimestamp)?;
        Ok(self.block_info.timestamp)
    }

    fn origin(&mut self) -> Result<gear_core::ids::ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Origin)?;
        Ok(self.origin)
    }

    fn send_commit(&mut self, handle: usize, msg: HandlePacket) -> Result<MessageId, Self::Error> {
        if 0 < msg.value() && msg.value() < self.existential_deposit {
            return self.return_and_store_err(Err(ExtError::InsufficientMessageValue));
        };

        self.charge_gas_runtime(RuntimeCosts::SendCommit(msg.payload().len() as u32))?;

        if self.gas_counter.reduce(msg.gas_limit().unwrap_or(0)) != ChargeResult::Enough {
            return self.return_and_store_err(Err(ExtError::GasLimitExceeded));
        };

        if self.value_counter.reduce(msg.value()) != ChargeResult::Enough {
            return self.return_and_store_err(Err(ExtError::NotEnoughValue));
        };

        let result = self
            .message_context
            .send_commit(handle as u32, msg)
            .map_err(ExtError::Message);

        self.return_and_store_err(result)
    }

    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Reply(msg.payload().len() as u32))?;
        if 0 < msg.value() && msg.value() < self.existential_deposit {
            return self.return_and_store_err(Err(ExtError::InsufficientMessageValue));
        };

        if self.value_counter.reduce(msg.value()) != ChargeResult::Enough {
            return self.return_and_store_err(Err(ExtError::NotEnoughValue));
        };

        let result = self
            .message_context
            .reply_commit(msg)
            .map_err(ExtError::Message);

        self.return_and_store_err(result)
    }

    fn exit(&mut self, value_destination: ProgramId) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Exit)?;
        if self.exit_argument.is_some() {
            Err(ExtError::ExitTwice)
        } else {
            self.exit_argument = Some(value_destination);
            Ok(())
        }
    }

    fn program_id(&mut self) -> Result<gear_core::ids::ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ProgramId)?;
        Ok(self.program_id)
    }

    fn free(&mut self, page: WasmPageNumber) -> Result<(), Self::Error> {
        let result = self.allocations_context.free(page).map_err(ExtError::Free);

        // Returns back gas for allocated page if it's new
        if !self.allocations_context.is_init_page(page) {
            self.refund_gas(self.config.alloc_cost as u32)?;
        }

        self.return_and_store_err(result)
    }

    fn debug(&mut self, data: &str) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Debug)?;

        if data.starts_with("panic occurred") {
            self.error_explanation = Some(ExtError::PanicOccurred);
        }
        log::debug!(target: "gwasm", "DEBUG: {}", data);

        Ok(())
    }

    fn create_program(&mut self, packet: InitPacket) -> Result<ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::CreateProgram)?;
        let code_hash = packet.code_id();

        // Send a message for program creation
        let result = self
            .message_context
            .init_program(packet)
            .map(|(new_prog_id, init_msg_id)| {
                // Save a program candidate for this run
                let entry = self.program_candidates_data.entry(code_hash).or_default();
                entry.push((new_prog_id, init_msg_id));

                new_prog_id
            })
            .map_err(ExtError::InitMessageNotDuplicated);

        self.return_and_store_err(result)
    }

    fn gas_counter(&mut self) -> &mut GasCounter {
        &mut self.gas_counter
    }

    fn gas_allowance_counter(&mut self) -> &mut GasAllowanceCounter {
        &mut self.gas_allowance_counter
    }

    fn value_counter(&mut self) -> &mut ValueCounter {
        &mut self.value_counter
    }

    fn message_context(&mut self) -> &mut MessageContext {
        &mut self.message_context
    }

    fn allocations_context(&mut self) -> &mut AllocationsContext {
        &mut self.allocations_context
    }

    fn host_fn_weights(&self) -> &HostFnWeights {
        &self.host_fn_weights
    }

    fn return_and_store_err<T>(&mut self, result: Result<T, ExtError>) -> Result<T, Self::Error> {
        result.map_err(|err| {
            self.error_explanation = Some(err);
            err
        })
    }
}
