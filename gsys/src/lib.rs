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

type Result<T, E = Len> = core::result::Result<T, E>;

/// Safe wrapper for `gr_block_height` syscall.
pub fn block_height() -> BlockNumber {
    let mut height = 0;

    unsafe { externs::gr_block_height(&mut height as *mut u32) }

    height
}

/// Safe wrapper for `gr_block_timestamp` syscall.
pub fn block_timestamp() -> BlockTimestamp {
    let mut timestamp = 0;

    unsafe { externs::gr_block_timestamp(&mut timestamp as *mut u64) }

    timestamp
}

/// Safe wrapper for `gr_create_program_wgas` syscall.
pub fn create_program_wgas(
    code_id: Hash,
    salt: &[u8],
    payload: &[u8],
    gas_limit: Gas,
    value: Value,
    delay: BlockNumber,
) -> Result<(Hash, Hash)> {
    let cid_value = (code_id, value);
    let mut mid_pid_err = ([0; 32], [0; 32], 0);

    unsafe {
        externs::gr_create_program_wgas(
            &cid_value as *const (Hash, Value),
            salt.as_ptr(),
            salt.len() as u32,
            payload.as_ptr(),
            payload.len() as u32,
            gas_limit,
            delay,
            &mut mid_pid_err as *mut (Hash, Hash, Len),
        )
    }

    match mid_pid_err {
        (mid, pid, 0) => Ok((mid, pid)),
        (_, _, len) => Err(len),
    }
}

/// Safe wrapper for `gr_create_program` syscall.
pub fn create_program(
    code_id: Hash,
    salt: &[u8],
    payload: &[u8],
    value: Value,
    delay: BlockNumber,
) -> Result<(Hash, Hash)> {
    let mut cid_value = [0; ]; //
    let cid_value = (code_id, value);
    let mut mid_pid_err = ([0; 32], [0; 32], 0);

    unsafe {
        externs::gr_create_program(
            &cid_value as *const (Hash, Value),
            salt.as_ptr(),
            salt.len() as u32,
            payload.as_ptr(),
            payload.len() as u32,
            delay,
            &mut mid_pid_err as *mut (Hash, Hash, Len),
        )
    }

    match mid_pid_err {
        (mid, pid, 0) => Ok((mid, pid)),
        (_, _, len) => Err(len),
    }
}

/// Safe wrapper for `gr_debug` syscall.
pub fn debug(payload: &[u8]) {
    unsafe { externs::gr_debug(payload.as_ptr(), payload.len() as u32) }
}

/// Safe wrapper for `gr_error` syscall.
pub fn error(error_with_len: &mut [u8]) -> Result<()> {
    unsafe { externs::gr_error(error_with_len.as_mut_ptr() as *mut (u8, u32)) };

    let mut err = [0; 4];
    err[..].copy_from_slice(&error_with_len[error_with_len.len() - 4..]);
    let err = u32::from_le_bytes(err);

    if err == 0 {
        Ok(())
    } else {
        Err(err)
    }
}
