extern crate alloc;

pub use alloc::{
    borrow::ToOwned,
    boxed::Box,
    collections::VecDeque,
    format,
    str::FromStr,
    string::{String, ToString},
    vec,
    vec::Vec,
};
pub use core::{mem, panic, ptr};

#[global_allocator]
pub static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;
