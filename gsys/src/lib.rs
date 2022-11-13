#![no_std]

pub type BlockNumber = u32;
pub type BlockTimestamp = u64;
pub type Bytes = u8;
pub type ExitCode = i32;
pub type Gas = u64;
pub type Handle = u32;
pub type Hash = [u8; 32];
pub type Len = u32;
pub type Value = u128;

pub mod externs;

// type Result<T, E = Len> = core::result::Result<T, E>;
