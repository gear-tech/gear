// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#[cfg(not(feature = "lazy-pages"))]
pub(crate) type Ext = core_processor::Ext;

#[cfg(feature = "lazy-pages")]
pub(crate) type Ext = lazy_pages::LazyPagesExt;

#[cfg(feature = "lazy-pages")]
mod lazy_pages {
    use actor_system_error::actor_system_error;
    use alloc::{
        collections::{BTreeMap, BTreeSet},
        vec::Vec,
    };
    use core_processor::{
        AllocExtError, Ext, FallibleExtError, ProcessorContext, ProcessorExternalities,
        UnrecoverableExtError,
    };
    use gear_backend_common::{
        lazy_pages::{GlobalsAccessConfig, LazyPagesWeights, Status},
        memory::ProcessAccessError,
        ActorTerminationReason, BackendExternalities, ExtInfo, TrapExplanation,
    };
    use gear_core::{
        costs::RuntimeCosts,
        env::{Externalities, PayloadSliceLock, UnlockPayloadBound},
        gas::{ChargeError, CounterType, CountersOwner, GasAmount, GasLeft},
        ids::{MessageId, ProgramId, ReservationId},
        memory::{GrowHandler, Memory, MemoryError, MemoryInterval, PageBuf},
        message::{HandlePacket, InitPacket, ReplyPacket},
        pages::{GearPage, PageU32Size, WasmPage},
    };
    use gear_core_errors::{ReplyCode, SignalCode};
    use gear_lazy_pages_common as lazy_pages;
    use gear_wasm_instrument::syscalls::SysCallName;

    actor_system_error! {
        pub type LazyPagesError = ActorSystemError<ActorLazyPagesError, SystemLazyPagesError>;
    }

    #[derive(Debug, Clone, Eq, PartialEq, derive_more::Display)]
    pub enum ActorLazyPagesError {}

    #[derive(Debug, Clone, Eq, PartialEq, derive_more::Display)]
    pub enum SystemLazyPagesError {
        /// Initial pages data must be empty in lazy pages mode
        #[display(fmt = "Initial pages data must be empty when execute with lazy pages")]
        InitialPagesContainsData,
    }

    pub struct LazyPagesInitContext {
        globals_config: GlobalsAccessConfig,
        lazy_pages_weights: LazyPagesWeights,
    }

    /// Ext with lazy pages support.
    pub struct LazyPagesExt {
        inner: Ext,
    }

    impl BackendExternalities for LazyPagesExt {
        fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, MemoryError> {
            let pages_for_data =
                |static_pages: WasmPage, allocations: &BTreeSet<WasmPage>| -> Vec<GearPage> {
                    // Accessed pages are all pages, that had been released and are in allocations set or static.
                    let mut accessed_pages = lazy_pages::get_write_accessed_pages();
                    accessed_pages.retain(|p| {
                        let wasm_page = p.to_page();
                        wasm_page < static_pages || allocations.contains(&wasm_page)
                    });
                    log::trace!("accessed pages numbers = {:?}", accessed_pages);
                    accessed_pages
                };
            self.inner.into_ext_info_inner(memory, pages_for_data)
        }

        fn gas_amount(&self) -> GasAmount {
            self.inner.context.gas_counter.to_amount()
        }

        fn pre_process_memory_accesses(
            reads: &[MemoryInterval],
            writes: &[MemoryInterval],
            gas_counter: &mut u64,
        ) -> Result<(), ProcessAccessError> {
            lazy_pages::pre_process_memory_accesses(reads, writes, gas_counter)
        }
    }

    impl ProcessorExternalities for LazyPagesExt {
        type ActorPagesError = ActorLazyPagesError;
        type SystemPagesError = SystemLazyPagesError;
        type PagesInitContext = LazyPagesInitContext;

        fn new(context: ProcessorContext) -> Self {
            Self {
                inner: Ext::new(context),
            }
        }

        fn pages_to_be_updated(
            _old_pages_data: BTreeMap<GearPage, PageBuf>,
            new_pages_data: BTreeMap<GearPage, PageBuf>,
            _static_pages: WasmPage,
        ) -> BTreeMap<GearPage, PageBuf> {
            // In lazy pages mode we update some page data in storage,
            // when it has been write accessed, so no need to compare old and new page data.
            new_pages_data.keys().for_each(|page| {
                log::trace!("{:?} has been write accessed, update it in storage", page)
            });
            new_pages_data
        }

        fn check_init_pages_data(
            initial_pages_data: &BTreeMap<GearPage, PageBuf>,
        ) -> Result<(), Self::SystemPagesError> {
            initial_pages_data
                .is_empty()
                .then_some(())
                .ok_or(SystemLazyPagesError::InitialPagesContainsData)
                .map_err(Into::into)
        }

        fn pages_init_context(
            &self,
            globals_config: GlobalsAccessConfig,
        ) -> Self::PagesInitContext {
            LazyPagesInitContext {
                globals_config,
                lazy_pages_weights: self.inner.context.page_costs.lazy_pages_weights(),
            }
        }

        fn init_pages_for_program(
            mem: &mut impl Memory,
            prog_id: ProgramId,
            stack_end: Option<WasmPage>,
            pages_data: &mut BTreeMap<GearPage, PageBuf>,
            _static_pages: WasmPage,
            ctx: Self::PagesInitContext,
        ) -> Result<(), LazyPagesError> {
            let LazyPagesInitContext {
                globals_config,
                lazy_pages_weights,
            } = ctx;

            Self::check_init_pages_data(pages_data)?;

            lazy_pages::init_for_program(
                mem,
                prog_id,
                stack_end,
                globals_config,
                lazy_pages_weights,
            );

            Ok(())
        }

        fn pages_post_execution_actions(
            mem: &mut impl Memory,
            termination: &mut ActorTerminationReason,
        ) {
            // released pages initial data will be added to `pages_initial_data`
            lazy_pages::remove_lazy_pages_prot(mem);

            match lazy_pages::get_status() {
                Status::Normal => (),
                Status::GasLimitExceeded => {
                    *termination = ActorTerminationReason::Trap(TrapExplanation::GasLimitExceeded);
                }
                Status::GasAllowanceExceeded => {
                    *termination = ActorTerminationReason::GasAllowanceExceeded;
                }
            }
        }
    }

    struct LazyGrowHandler {
        old_mem_addr: Option<u64>,
        old_mem_size: WasmPage,
    }

    impl GrowHandler for LazyGrowHandler {
        fn before_grow_action(mem: &mut impl Memory) -> Self {
            // New pages allocation may change wasm memory buffer location.
            // So we remove protections from lazy-pages
            // and then in `after_grow_action` we set protection back for new wasm memory buffer.
            let old_mem_addr = mem.get_buffer_host_addr();
            lazy_pages::remove_lazy_pages_prot(mem);
            Self {
                old_mem_addr,
                old_mem_size: mem.size(),
            }
        }

        fn after_grow_action(self, mem: &mut impl Memory) {
            // Add new allocations to lazy pages.
            // Protect all lazy pages including new allocations.
            let new_mem_addr = mem.get_buffer_host_addr().unwrap_or_else(|| {
                unreachable!("Memory size cannot be zero after grow is applied for memory")
            });
            lazy_pages::update_lazy_pages_and_protect_again(
                mem,
                self.old_mem_addr,
                self.old_mem_size,
                new_mem_addr,
            );
        }
    }

    impl CountersOwner for LazyPagesExt {
        fn charge_gas_runtime(&mut self, cost: RuntimeCosts) -> Result<(), ChargeError> {
            self.inner.charge_gas_runtime(cost)
        }

        fn charge_gas_runtime_if_enough(&mut self, cost: RuntimeCosts) -> Result<(), ChargeError> {
            self.inner.charge_gas_runtime_if_enough(cost)
        }

        fn charge_gas_if_enough(&mut self, amount: u64) -> Result<(), ChargeError> {
            self.inner.charge_gas_if_enough(amount)
        }

        fn gas_left(&self) -> GasLeft {
            self.inner.gas_left()
        }

        fn current_counter_type(&self) -> CounterType {
            self.inner.current_counter_type()
        }

        fn decrease_current_counter_to(&mut self, amount: u64) {
            self.inner.decrease_current_counter_to(amount)
        }

        fn define_current_counter(&mut self) -> u64 {
            self.inner.define_current_counter()
        }
    }

    impl Externalities for LazyPagesExt {
        type UnrecoverableError = UnrecoverableExtError;
        type FallibleError = FallibleExtError;
        type AllocError = AllocExtError;

        fn alloc(
            &mut self,
            pages_num: u32,
            mem: &mut impl Memory,
        ) -> Result<WasmPage, Self::AllocError> {
            self.inner.alloc_inner::<LazyGrowHandler>(pages_num, mem)
        }

        fn free(&mut self, page: WasmPage) -> Result<(), Self::AllocError> {
            self.inner.free(page)
        }

        fn block_height(&self) -> Result<u32, Self::UnrecoverableError> {
            self.inner.block_height()
        }

        fn block_timestamp(&self) -> Result<u64, Self::UnrecoverableError> {
            self.inner.block_timestamp()
        }

        fn send_init(&mut self) -> Result<u32, Self::FallibleError> {
            self.inner.send_init()
        }

        fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Self::FallibleError> {
            self.inner.send_push(handle, buffer)
        }

        fn send_push_input(
            &mut self,
            handle: u32,
            offset: u32,
            len: u32,
        ) -> Result<(), Self::FallibleError> {
            self.inner.send_push_input(handle, offset, len)
        }

        fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::FallibleError> {
            self.inner.reply_push(buffer)
        }

        fn send_commit(
            &mut self,
            handle: u32,
            msg: HandlePacket,
            delay: u32,
        ) -> Result<MessageId, Self::FallibleError> {
            self.inner.send_commit(handle, msg, delay)
        }

        fn reservation_send_commit(
            &mut self,
            id: ReservationId,
            handle: u32,
            msg: HandlePacket,
            delay: u32,
        ) -> Result<MessageId, Self::FallibleError> {
            self.inner.reservation_send_commit(id, handle, msg, delay)
        }

        fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::FallibleError> {
            self.inner.reply_commit(msg)
        }

        fn reservation_reply_commit(
            &mut self,
            id: ReservationId,
            msg: ReplyPacket,
        ) -> Result<MessageId, Self::FallibleError> {
            self.inner.reservation_reply_commit(id, msg)
        }

        fn reply_to(&self) -> Result<MessageId, Self::FallibleError> {
            self.inner.reply_to()
        }

        fn signal_from(&self) -> Result<MessageId, Self::FallibleError> {
            self.inner.signal_from()
        }

        fn reply_push_input(&mut self, offset: u32, len: u32) -> Result<(), Self::FallibleError> {
            self.inner.reply_push_input(offset, len)
        }

        fn source(&self) -> Result<ProgramId, Self::UnrecoverableError> {
            self.inner.source()
        }

        fn reply_code(&self) -> Result<ReplyCode, Self::FallibleError> {
            self.inner.reply_code()
        }

        fn signal_code(&self) -> Result<SignalCode, Self::FallibleError> {
            self.inner.signal_code()
        }

        fn message_id(&self) -> Result<MessageId, Self::UnrecoverableError> {
            self.inner.message_id()
        }

        fn pay_program_rent(
            &mut self,
            program_id: ProgramId,
            rent: u128,
        ) -> Result<(u128, u32), Self::FallibleError> {
            self.inner.pay_program_rent(program_id, rent)
        }

        fn program_id(&self) -> Result<ProgramId, Self::UnrecoverableError> {
            self.inner.program_id()
        }

        fn debug(&self, data: &str) -> Result<(), Self::UnrecoverableError> {
            self.inner.debug(data)
        }

        fn size(&self) -> Result<usize, Self::UnrecoverableError> {
            self.inner.size()
        }

        fn random(&self) -> Result<(&[u8], u32), Self::UnrecoverableError> {
            self.inner.random()
        }

        fn reserve_gas(
            &mut self,
            amount: u64,
            blocks: u32,
        ) -> Result<ReservationId, Self::FallibleError> {
            self.inner.reserve_gas(amount, blocks)
        }

        fn unreserve_gas(&mut self, id: ReservationId) -> Result<u64, Self::FallibleError> {
            self.inner.unreserve_gas(id)
        }

        fn system_reserve_gas(&mut self, amount: u64) -> Result<(), Self::FallibleError> {
            self.inner.system_reserve_gas(amount)
        }

        fn gas_available(&self) -> Result<u64, Self::UnrecoverableError> {
            self.inner.gas_available()
        }

        fn value(&self) -> Result<u128, Self::UnrecoverableError> {
            self.inner.value()
        }

        fn wait(&mut self) -> Result<(), Self::UnrecoverableError> {
            self.inner.wait()
        }

        fn wait_for(&mut self, duration: u32) -> Result<(), Self::UnrecoverableError> {
            self.inner.wait_for(duration)
        }

        fn wait_up_to(&mut self, duration: u32) -> Result<bool, Self::UnrecoverableError> {
            self.inner.wait_up_to(duration)
        }

        fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Self::FallibleError> {
            self.inner.wake(waker_id, delay)
        }

        fn value_available(&self) -> Result<u128, Self::UnrecoverableError> {
            self.inner.value_available()
        }

        fn create_program(
            &mut self,
            packet: InitPacket,
            delay: u32,
        ) -> Result<(MessageId, ProgramId), Self::FallibleError> {
            self.inner.create_program(packet, delay)
        }

        fn reply_deposit(
            &mut self,
            message_id: MessageId,
            amount: u64,
        ) -> Result<(), Self::FallibleError> {
            self.inner.reply_deposit(message_id, amount)
        }

        fn forbidden_funcs(&self) -> &BTreeSet<SysCallName> {
            &self.inner.context.forbidden_funcs
        }

        fn lock_payload(
            &mut self,
            at: u32,
            len: u32,
        ) -> Result<PayloadSliceLock, Self::FallibleError> {
            self.inner.lock_payload(at, len)
        }

        fn unlock_payload(&mut self, payload_holder: &mut PayloadSliceLock) -> UnlockPayloadBound {
            self.inner.unlock_payload(payload_holder)
        }
    }

    #[cfg(test)]
    mod tests {
        use crate::ext::lazy_pages::LazyPagesExt;
        use alloc::collections::BTreeMap;
        use core_processor::ProcessorExternalities;
        use gear_core::{
            memory::{PageBuf, PageBufInner},
            pages::GearPage,
        };

        #[test]
        fn lazy_pages_to_update() {
            let new_pages: BTreeMap<_, _> = [
                (
                    GearPage::from(0xBABE),
                    PageBuf::from_inner(PageBufInner::filled_with(123)),
                ),
                (
                    GearPage::from(0xCAFE),
                    PageBuf::from_inner(PageBufInner::filled_with(254)),
                ),
            ]
            .into();
            let res =
                LazyPagesExt::pages_to_be_updated(Default::default(), new_pages.clone(), 0.into());
            // All touched pages are to be updated in lazy mode
            assert_eq!(res, new_pages);
        }
    }
}
