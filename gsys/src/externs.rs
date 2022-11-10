use crate::*;

#[allow(improper_ctypes)]
extern "C" {
    /// Infallible `gr_block_height` get syscall.
    pub fn gr_block_height(height: *mut BlockNumber);

    /// Infallible `gr_block_timestamp` get syscall.
    pub fn gr_block_timestamp(timestamp: *mut BlockTimestamp);

    /// Fallible `gr_create_program_wgas` send syscall.
    ///
    /// Arguments naming:
    /// * cid: code id
    /// * mid: message id
    /// * pid: program id
    pub fn gr_create_program_wgas(
        cid_value: *const (Hash, Value),
        salt: *const Bytes,
        salt_len: Len,
        payload: *const Bytes,
        payload_len: Len,
        gas_limit: Gas,
        delay: BlockNumber,
        mid_pid_err: *mut (Hash, Hash, Len),
    );

    /// Fallible `gr_create_program` send syscall.
    ///
    /// Arguments naming:
    /// * cid: code id
    /// * mid: message id
    /// * pid: program id
    pub fn gr_create_program(
        cid_value: *const (Hash, Value),
        salt: *const Bytes,
        salt_len: Len,
        payload: *const Bytes,
        payload_len: Len,
        delay: BlockNumber,
        mid_pid_err: *mut (Hash, Hash, Len),
    );

    /// Infallible `gr_debug` info syscall.
    pub fn gr_debug(payload: *const Bytes, len: Len);

    /// Fallible `gr_error` get syscall.
    pub fn gr_error(error_err: *mut (Bytes, Len));

    /// Fallible `gr_exit_code` get syscall.
    pub fn gr_exit_code(code_err: *mut (ExitCode, Len));

    /// Infallible `gr_exit` control syscall.
    pub fn gr_exit(inheritor_id: *const Hash);

    /// Infallible `gr_gas_available` get syscall.
    pub fn gr_gas_available(gas_limit: *mut Gas);

    /// Infallible `gr_leave` control syscall.
    pub fn gr_leave();

    /// Infallible `gr_message_id` get syscall.
    pub fn gr_message_id(message_id: *mut Hash);

    /// Infallible `gr_origin` get syscall.
    pub fn gr_origin(program_id: *mut Hash);

    /// Infallible `gr_program_id` get syscall.
    pub fn gr_program_id(program_id: *mut Hash);

    /// Infallible `gr_random` calculate syscall.
    ///
    /// Arguments naming:
    /// * bn: block number
    pub fn gr_random(subject: *const Bytes, len: Len, random_bn: *mut (Hash, BlockNumber));

    /// Fallible `gr_read` get syscall.
    pub fn gr_read(at: Len, len: Len, buffer_err: *mut (Bytes, Len));

    /// Fallible `gr_reply_commit_wgas` send syscall.
    ///
    /// Arguments naming:
    /// * mid: message id
    pub fn gr_reply_commit_wgas(
        gas_limit: Gas,
        value: *const Value,
        delay: BlockNumber,
        mid_err: *mut (Hash, Len),
    );

    /// Fallible `gr_reply_commit` send syscall.
    ///
    /// Arguments naming:
    /// * mid: message id
    pub fn gr_reply_commit(value: *const Value, delay: BlockNumber, mid_err: *mut (Hash, Len));

    /// Fallible `gr_reply_push` send syscall.
    pub fn gr_reply_push(payload: *const Bytes, len: Len, err: *mut Len);

    /// Fallible `gr_reply_to` get syscall.
    ///
    /// Arguments naming:
    /// * mid: message id
    pub fn gr_reply_to(mid_err: *mut (Hash, Len));

    /// Fallible `gr_reply_wgas` send syscall.
    ///
    /// Arguments naming:
    /// * mid: message id
    pub fn gr_reply_wgas(
        payload: *const Bytes,
        payload_len: Len,
        gas_limit: Gas,
        value: *const Value,
        delay: BlockNumber,
        mid_err: *mut (Hash, Len),
    );

    /// Fallible `gr_reply` send syscall.
    ///
    /// Arguments naming:
    /// * mid: message id
    pub fn gr_reply(
        payload: *const Bytes,
        payload_len: Len,
        value: *const Value,
        delay: BlockNumber,
        mid_err: *mut (Hash, Len),
    );

    /// Fallible `gr_reserve_gas` control syscall.
    ///
    /// Arguments naming:
    /// * rid: reservation id
    pub fn gr_reserve_gas(gas_limit: Gas, duration: BlockNumber, rid_err: *mut (Hash, Len));

    /// Fallible `gr_send_commit_wgas` send syscall.
    ///
    /// Arguments naming:
    /// * pid: program id
    /// * mid: message id
    pub fn gr_send_commit_wgas(
        handle: Handle,
        pid_value: *const (Hash, Value),
        gas_limit: Gas,
        delay: BlockNumber,
        mid_err: *mut (Hash, Len),
    );

    /// Fallible `gr_send_commit` send syscall.
    ///
    /// Arguments naming:
    /// * pid: program id
    /// * mid: message id
    pub fn gr_send_commit(
        handle: Handle,
        pid_value: *const (Hash, Value),
        delay: BlockNumber,
        mid_err: *mut (Hash, Len),
    );

    /// Fallible `gr_send_init` send syscall.
    pub fn gr_send_init(handle_err: *mut (Handle, Len));

    /// Fallible `gr_send_push` send syscall.
    pub fn gr_send_push(handle: Handle, payload: *const Bytes, len: Len, err: *mut Len);

    /// Fallible `gr_send_wgas` send syscall.
    ///
    /// Arguments naming:
    /// * pid: program id
    /// * mid: message id
    pub fn gr_send_wgas(
        pid_value: *const (Hash, Value),
        payload: *const Bytes,
        payload_len: Len,
        gas_limit: Gas,
        delay: BlockNumber,
        mid_err: *mut (Hash, Len),
    );

    /// Fallible `gr_send` send syscall.
    ///
    /// Arguments naming:
    /// * pid: program id
    /// * mid: message id
    pub fn gr_send(
        pid_value: *const (Hash, Value),
        payload: *const Bytes,
        payload_len: Len,
        delay: BlockNumber,
        mid_err: *mut (Hash, Len),
    );

    /// Infallible `gr_size` get syscall.
    pub fn gr_size(len: *mut Len);

    /// Infallible `gr_source` get syscall.
    pub fn gr_source(program_id: *mut Hash);

    /// Fallible `gr_unreserve_gas` control syscall.
    ///
    /// Arguments naming:
    /// * un: unreserved amount
    pub fn gr_unreserve_gas(reservation_id: *const Hash, un_err: *mut (Gas, Len));

    /// Infallible `gr_value_available` get syscall.
    pub fn gr_value_available(value: *mut Value);

    /// Infallible `gr_value` get syscall.
    pub fn gr_value(value: *mut Value);

    /// Infallible `gr_wait_for` control syscall.
    pub fn gr_wait_for(duration: BlockNumber);

    /// Infallible `gr_wait_up_to` control syscall.
    pub fn gr_wait_up_to(duration: BlockNumber);

    /// Infallible `gr_wait` control syscall.
    pub fn gr_wait();

    /// Fallible `gr_wake` control syscall.
    pub fn gr_wake(message_id: *const Hash, delay: BlockNumber, err: *mut Len);
}
