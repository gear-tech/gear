use core::mem::MaybeUninit;

use crate::{ActorId, MessageId};
use alloc::vec;
use gsys::stack_buffer::{get_stack_buffer_global, set_stack_buffer_global, StackBuffer};

enum HostGetterIndex {
    BlockHeight = 32,
    BlockTimestamp,
    MessageId,
    Origin,
    MessageSize,
    StatusCode,
    Source,
    Value,
}

fn call_const_getter<T: Clone + From<K>, K: Clone + From<T>>(
    index: HostGetterIndex,
    get: impl FnOnce() -> T,
    stack_buffer_field: impl FnOnce(&mut StackBuffer) -> &mut K,
) -> T {
    unsafe {
        let mut flags = get_stack_buffer_global();
        let stack_buffer_offset = (flags & (u32::MAX as u64)) as usize;
        if stack_buffer_offset == 0 {
            return get();
        }

        let stack_buffer = (stack_buffer_offset as *mut StackBuffer).as_mut().unwrap();

        let mask = 1u64 << index as u64;

        if flags & mask != 0 {
            stack_buffer_field(stack_buffer).clone().into()
        } else {
            let data = get();
            *stack_buffer_field(stack_buffer) = data.clone().into();
            flags |= mask;
            set_stack_buffer_global(flags);
            data
        }
    }
}

#[inline(never)]
fn with_byte_array<T, const N: usize>(size: usize, f: impl FnOnce(&mut [u8]) -> T) -> T {
    let mut buffer = [0u8; N];
    let sub_buffer = &mut buffer[0..size];
    f(sub_buffer)
}

pub fn with_byte_buffer<T>(size: usize, f: impl FnOnce(&mut [u8]) -> T) -> T {
    match size {
        size if size <= 0x1 => with_byte_array::<_, 0x1>(size, f),
        size if size <= 0x2 => with_byte_array::<_, 0x2>(size, f),
        size if size <= 0x4 => with_byte_array::<_, 0x4>(size, f),
        size if size <= 0x8 => with_byte_array::<_, 0x8>(size, f),
        size if size <= 0x10 => with_byte_array::<_, 0x10>(size, f),
        size if size <= 0x20 => with_byte_array::<_, 0x20>(size, f),
        size if size <= 0x40 => with_byte_array::<_, 0x40>(size, f),
        size if size <= 0x80 => with_byte_array::<_, 0x80>(size, f),
        size if size <= 0x100 => with_byte_array::<_, 0x100>(size, f),
        size if size <= 0x200 => with_byte_array::<_, 0x200>(size, f),
        size if size <= 0x400 => with_byte_array::<_, 0x400>(size, f),
        size if size <= 0x800 => with_byte_array::<_, 0x800>(size, f),
        size if size <= 0x1000 => with_byte_array::<_, 0x1000>(size, f),
        size if size <= 0x2000 => with_byte_array::<_, 0x2000>(size, f),
        size if size <= 0x4000 => with_byte_array::<_, 0x4000>(size, f),
        _ => f(vec![0; size].as_mut_slice()),
    }
}

/// +_+_+
pub fn with_stack_buffer<T>(f: impl FnOnce() -> T) -> T {
    let uninit = MaybeUninit::<StackBuffer>::uninit();
    let stack_buffer = unsafe { uninit.assume_init() };
    let stack_buffer_offset = &stack_buffer as *const StackBuffer as usize;
    let mut global = unsafe { get_stack_buffer_global() };
    global |= stack_buffer_offset as u64;
    unsafe { set_stack_buffer_global(global) };
    f()
}

pub fn origin() -> ActorId {
    call_const_getter(
        HostGetterIndex::Origin,
        crate::exec::origin_syscall_wrapper,
        |stack_buffer| &mut stack_buffer.origin,
    )
}

pub fn size() -> usize {
    call_const_getter(
        HostGetterIndex::MessageSize,
        crate::msg::size_syscall_wrapper,
        |stack_buffer| &mut stack_buffer.message_size,
    )
}

pub fn message_id() -> MessageId {
    call_const_getter(
        HostGetterIndex::MessageId,
        crate::msg::message_id_syscall_wrapper,
        |stack_buffer| &mut stack_buffer.message_id,
    )
}

pub fn block_height() -> u32 {
    call_const_getter(
        HostGetterIndex::BlockHeight,
        crate::exec::block_height_syscall_wrapper,
        |stack_buffer| &mut stack_buffer.block_height,
    )
}

pub fn block_timestamp() -> u64 {
    call_const_getter(
        HostGetterIndex::BlockTimestamp,
        crate::exec::block_timestamp_syscall_wrapper,
        |stack_buffer| &mut stack_buffer.block_timestamp,
    )
}

pub fn status_code() -> gsys::LengthWithCode {
    call_const_getter(
        HostGetterIndex::StatusCode,
        crate::msg::status_code_syscall_wrapper,
        |stack_buffer| &mut stack_buffer.status_code,
    )
}

pub fn source() -> ActorId {
    call_const_getter(
        HostGetterIndex::Source,
        crate::msg::source_syscall_wrapper,
        |stack_buffer| &mut stack_buffer.source,
    )
}

pub fn value() -> u128 {
    call_const_getter(
        HostGetterIndex::Value,
        crate::msg::value_syscall_wrapper,
        |stack_buffer| &mut stack_buffer.value,
    )
}
