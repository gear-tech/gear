// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// See `THIRD_PARTY_NOTICES.md` for the upstream source and license reference.

/// The error type used by the allocators.
#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    /// Someone tried to allocate more memory than the allowed maximum per allocation.
    #[error("Requested allocation size is too large")]
    RequestedAllocationTooLarge,
    /// Allocator run out of space.
    #[error("Allocator ran out of space")]
    AllocatorOutOfSpace,
    /// The client passed a memory instance which is smaller than previously observed.
    #[error("Shrinking of the underlying memory is observed")]
    MemoryShrunk,
    /// Some other error occurred.
    #[error("Other: {0}")]
    Other(&'static str),
}
