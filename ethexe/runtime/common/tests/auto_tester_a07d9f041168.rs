// auto_tester_a07d9f041168
// scenario: invalid_combination
// param_signature: remove_and_store_regions stale region hash on full region clear

// Reproduces the bug tracked by `// TODO #5373` at ethexe/runtime/common/src/state.rs:1057-1062.
// Closed upstream issue: https://github.com/gear-tech/gear/issues/5373
//
// When `remove_and_store_regions` empties a region (last page removed),
// `region.store(storage)` returns `MaybeHashOf::empty()`. The if-let in the
// update loop then never fires, so `self[region_idx]` keeps its old
// (non-empty) hash — the MemoryPages aggregate hash is stale.
//
// Expected: removing the only page in a region must change the MemoryPages
// hash (the region is no longer represented by its old content hash).
// Observed: hash before == hash after.

use ethexe_runtime_common::state::{MemStorage, MemoryPages, Storage};
use gear_core::{memory::PageBuf, pages::GearPage};
use std::collections::BTreeMap;

#[test]
fn remove_last_page_in_region_must_clear_region_hash() {
    let storage = MemStorage::default();
    let mut pages = MemoryPages::default();

    // Add one page to region 0.
    let page0 = GearPage::from(0u16);
    let buf = PageBuf::new_zeroed();
    let page_hash = storage.write_page_data(buf);

    let mut page_map = BTreeMap::new();
    page_map.insert(page0, page_hash);
    pages.update_and_store_regions(&storage, page_map);

    let hash_with_page = pages.clone().store(&storage).to_inner();

    // Remove the only page — region 0 is now empty.
    pages.remove_and_store_regions(&storage, &vec![page0]);

    let hash_after_remove = pages.clone().store(&storage).to_inner();

    // The aggregate hash MUST change: an empty region is observationally
    // different from a region holding one page.
    assert_ne!(
        hash_with_page, hash_after_remove,
        "MemoryPages hash must change when the last page in a region is removed; \
         see TODO #5373 in state.rs:1057-1062 (the if-let skips empty regions \
         and leaves a stale region hash in self[region_idx])"
    );
}
