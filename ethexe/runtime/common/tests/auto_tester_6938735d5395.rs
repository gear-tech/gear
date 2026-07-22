// scenario: zero_value
// param_signature: PayloadLookup::force_stored on empty payload should remain Direct
// hash: 6938735d5395

use ethexe_runtime_common::state::{MemStorage, PayloadLookup};

/// Invariant from state.rs:62 -- "Zero payload should always be stored directly."
/// force_stored on an empty payload must write the payload to storage, but
/// the invariant says empty payload must stay Direct.  We probe whether
/// calling force_stored on an empty Direct payload produces a Stored variant
/// (which would violate the documented invariant) or a Direct empty variant.
#[test]
fn zero_payload_force_stored_violates_invariant() {
    let storage = MemStorage::default();

    let mut lookup = PayloadLookup::empty();
    assert!(lookup.is_empty(), "should start empty");

    // After force_stored the value is always converted to Stored(hash).
    // But the doc says "Zero payload should always be stored directly."
    // So a Stored result for an empty payload is a violation.
    let _hash = lookup.force_stored(&storage);

    // If the invariant holds, `lookup` should still be Direct(empty).
    // If it became Stored, the invariant is broken.
    assert!(
        lookup.is_empty(),
        "Invariant violation: empty payload became Stored after force_stored; \
         'Zero payload should always be stored directly.' (state.rs:62)"
    );
}
