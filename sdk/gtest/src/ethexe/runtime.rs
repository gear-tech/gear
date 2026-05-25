// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::{HashOf, injected::Promise};
use ethexe_runtime_common::{
    RuntimeInterface,
    state::{
        ActiveProgram, Allocations, DispatchStash, Mailbox, MemStorage, MemoryPages,
        MemoryPagesRegion, MessageQueue, Program, ProgramState, QueryableStorage, Storage,
        UserMailbox, Waitlist,
    },
};
use gear_core::{buffer::Payload, ids::ActorId, memory::PageBuf, pages::GearPage};
use gear_lazy_pages::{LazyPagesStorage, LazyPagesVersion};
use gear_lazy_pages_common::LazyPagesInitContext;
use gprimitives::H256;
use parity_scale_codec::{Decode, DecodeAll};
use std::{cell::Cell, ptr};

const PAGE_STORAGE_PREFIX: [u8; 32] = *b"gtestethexe_lazy_pages_prefix000";

thread_local! {
    static LAZY_PAGES_STORAGE: Cell<*const MemStorage> = const { Cell::new(ptr::null()) };
    static LAZY_PAGES_STATE_HASH: Cell<H256> = const { Cell::new(H256::zero()) };
}

pub(crate) struct GTestEthexeRuntime {
    storage: *const MemStorage,
    state_hash: Cell<H256>,
}

impl GTestEthexeRuntime {
    pub(crate) fn new(storage: &MemStorage, state_hash: H256) -> Self {
        Self {
            storage,
            state_hash: Cell::new(state_hash),
        }
    }

    pub(crate) fn state_hash(&self) -> H256 {
        self.state_hash.get()
    }

    fn storage(&self) -> &MemStorage {
        // SAFETY: gtest ethexe execution is synchronous. The runtime is created
        // from the backend-owned MemStorage and is used only while that backend
        // is alive, so the raw pointer remains valid for the execution window.
        unsafe { &*self.storage }
    }
}

impl RuntimeInterface for GTestEthexeRuntime {
    type LazyPages = gear_lazy_pages_native_interface::LazyPagesNative;

    fn init_lazy_pages(&self) {
        LAZY_PAGES_STORAGE.set(self.storage);
        LAZY_PAGES_STATE_HASH.set(self.state_hash());
    }

    fn random_data(&self) -> (Vec<u8>, u32) {
        // Deterministic test entropy; wire this to BlocksManager when a test needs variability.
        (vec![42; 32], 0)
    }

    fn update_state_hash(&self, state_hash: &H256) {
        self.state_hash.set(*state_hash);
        LAZY_PAGES_STATE_HASH.set(*state_hash);
    }

    fn publish_promise(&self, _promise: &Promise) {}
}

impl Storage for GTestEthexeRuntime {
    fn program_state(&self, hash: H256) -> Option<ProgramState> {
        self.storage().program_state(hash)
    }

    fn write_program_state(&self, state: ProgramState) -> H256 {
        self.storage().write_program_state(state)
    }

    fn message_queue(&self, hash: HashOf<MessageQueue>) -> Option<MessageQueue> {
        self.storage().message_queue(hash)
    }

    fn write_message_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue> {
        self.storage().write_message_queue(queue)
    }

    fn waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist> {
        self.storage().waitlist(hash)
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist> {
        self.storage().write_waitlist(waitlist)
    }

    fn dispatch_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash> {
        self.storage().dispatch_stash(hash)
    }

    fn write_dispatch_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash> {
        self.storage().write_dispatch_stash(stash)
    }

    fn mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox> {
        self.storage().mailbox(hash)
    }

    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox> {
        self.storage().write_mailbox(mailbox)
    }

    fn user_mailbox(&self, hash: HashOf<UserMailbox>) -> Option<UserMailbox> {
        self.storage().user_mailbox(hash)
    }

    fn write_user_mailbox(&self, user_mailbox: UserMailbox) -> HashOf<UserMailbox> {
        self.storage().write_user_mailbox(user_mailbox)
    }

    fn memory_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages> {
        self.storage().memory_pages(hash)
    }

    fn memory_pages_region(&self, hash: HashOf<MemoryPagesRegion>) -> Option<MemoryPagesRegion> {
        self.storage().memory_pages_region(hash)
    }

    fn write_memory_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages> {
        self.storage().write_memory_pages(pages)
    }

    fn write_memory_pages_region(
        &self,
        pages_region: MemoryPagesRegion,
    ) -> HashOf<MemoryPagesRegion> {
        self.storage().write_memory_pages_region(pages_region)
    }

    fn allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations> {
        self.storage().allocations(hash)
    }

    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations> {
        self.storage().write_allocations(allocations)
    }

    fn payload(&self, hash: HashOf<Payload>) -> Option<Payload> {
        self.storage().payload(hash)
    }

    fn write_payload(&self, payload: Payload) -> HashOf<Payload> {
        self.storage().write_payload(payload)
    }

    fn page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf> {
        self.storage().page_data(hash)
    }

    fn write_page_data(&self, data: PageBuf) -> HashOf<PageBuf> {
        self.storage().write_page_data(data)
    }
}

#[derive(Debug)]
struct EthexePagesStorage;

impl EthexePagesStorage {
    fn storage(&self) -> &MemStorage {
        let storage = LAZY_PAGES_STORAGE.get();
        assert!(
            !storage.is_null(),
            "ethexe lazy-pages storage requested outside of gtest execution"
        );
        // SAFETY: the pointer is set from the backend-owned MemStorage before
        // synchronous runtime execution and cleared only by replacing it for a
        // later execution in the same gtest System lifetime.
        unsafe { &*storage }
    }

    fn state_hash(&self) -> H256 {
        LAZY_PAGES_STATE_HASH.get()
    }

    fn page_data_hash(&self, page: GearPage) -> Option<HashOf<PageBuf>> {
        let storage = self.storage();
        let ProgramState {
            program: Program::Active(ActiveProgram { pages_hash, .. }),
            ..
        } = storage.program_state(self.state_hash())?
        else {
            return None;
        };

        let pages: MemoryPages = storage.query(&pages_hash).ok()?;
        let region_hash = pages[MemoryPages::page_region(page)].to_inner()?;
        let region = storage.memory_pages_region(region_hash)?;

        region.as_inner().get(&page).copied()
    }
}

#[derive(Decode)]
struct PageKey {
    _page_storage_prefix: [u8; 32],
    _program_id: ActorId,
    _memory_infix: u32,
    page: GearPage,
}

impl PageKey {
    fn decode_page(mut key: &[u8]) -> GearPage {
        let PageKey { page, .. } = PageKey::decode_all(&mut key).expect("Invalid key");
        page
    }
}

impl LazyPagesStorage for EthexePagesStorage {
    fn page_exists(&self, key: &[u8]) -> bool {
        self.page_data_hash(PageKey::decode_page(key)).is_some()
    }

    fn load_page(&mut self, key: &[u8], buffer: &mut [u8]) -> Option<u32> {
        let page_hash = self.page_data_hash(PageKey::decode_page(key))?;
        let data = self.storage().page_data(page_hash)?;
        let len = data.len();

        buffer[..len].copy_from_slice(&data);

        Some(len as u32)
    }
}

pub(crate) fn init_lazy_pages() {
    gear_lazy_pages::init(
        LazyPagesVersion::Version1,
        LazyPagesInitContext::new(PAGE_STORAGE_PREFIX),
        EthexePagesStorage,
    )
    .expect("Failed to init ethexe lazy-pages");
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Encode;
    use std::collections::BTreeMap;

    fn state_with_page(storage: &MemStorage, page: GearPage, byte: u8) -> H256 {
        let page_hash = storage.write_page_data(PageBuf::filled_with(byte));
        let mut pages = MemoryPages::default();
        pages.update_and_store_regions(storage, BTreeMap::from([(page, page_hash)]));

        let mut state = ProgramState::zero();
        let Program::Active(ActiveProgram { pages_hash, .. }) = &mut state.program else {
            unreachable!("zero ethexe program state is active");
        };
        *pages_hash = pages.store(storage);

        storage.write_program_state(state)
    }

    #[test]
    fn lazy_pages_storage_loads_page_from_current_state_hash() {
        let storage = MemStorage::default();
        let page = GearPage::from(7);
        let old_hash = state_with_page(&storage, page, 1);
        let new_hash = state_with_page(&storage, page, 2);
        let runtime = GTestEthexeRuntime::new(&storage, old_hash);
        runtime.init_lazy_pages();
        let mut pages_storage = EthexePagesStorage;
        let key = (PAGE_STORAGE_PREFIX, ActorId::from(99), 0u32, page).encode();
        let mut buffer = vec![0; GearPage::SIZE as usize];

        assert!(pages_storage.page_exists(&key));
        assert_eq!(
            pages_storage.load_page(&key, &mut buffer),
            Some(GearPage::SIZE)
        );
        assert_eq!(buffer[0], 1);

        runtime.update_state_hash(&new_hash);

        assert_eq!(
            pages_storage.load_page(&key, &mut buffer),
            Some(GearPage::SIZE)
        );
        assert_eq!(buffer[0], 2);
    }
}
