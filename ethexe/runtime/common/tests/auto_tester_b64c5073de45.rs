// scenario: duplicate_key
// param_signature: Waitlist::wait twice with same dispatch.id triggers debug_assert failure
// hash: b64c5073de45

use ethexe_runtime_common::state::{Waitlist, Dispatch, PayloadLookup};
use ethexe_common::gear::MessageType;
use gprimitives::{ActorId, MessageId};
use gear_core::buffer::Payload;
use gear_core_errors::SuccessReplyReason;

#[test]
fn test_waitlist_wait_duplicate_id_debug_assert() {
    let mut waitlist = Waitlist::default();

    let dispatch = Dispatch::reply(
        MessageId::from(42),
        ActorId::from(1),
        PayloadLookup::Direct(Payload::new()),
        0,
        SuccessReplyReason::Auto,
        MessageType::Canonical,
        false,
    );

    // First wait — succeeds
    waitlist.wait(dispatch.clone(), 100);

    // Second wait with same id — debug_assert!(r.is_none()) fires in debug builds.
    // In release builds, the insert silently overwrites the previous entry.
    // Either way the behavior needs to be consistent: the key is deduplicated.
    waitlist.wait(dispatch.clone(), 200);

    // Check how many entries we have: should be 1 (second overwrote first) in release,
    // or this panics in debug due to debug_assert.
    let inner = waitlist.into_inner();
    assert_eq!(inner.len(), 1, "duplicate id should only have one entry");

    // The surviving entry should have expiry=200 (last write wins)
    let entry = inner.get(&MessageId::from(42)).expect("entry must exist");
    assert_eq!(entry.expiry, 200, "last write should win");
}
