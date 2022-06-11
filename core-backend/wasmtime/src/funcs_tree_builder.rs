use crate::{env::StoreData, funcs::FuncsHandler};
use alloc::collections::BTreeMap;
use gear_backend_common::{error_processor::IntoExtError, AsTerminationReason, IntoExtInfo};
use gear_core::env::Ext;
use wasmtime::{Func, Memory, Store};

pub fn get_funcs_tree<E>(store: &mut Store<StoreData<E>>, memory: Memory) -> BTreeMap<&str, Func>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    [
        ("alloc", FuncsHandler::alloc(store, memory)),
        ("free", FuncsHandler::free(store)),
        ("gas", FuncsHandler::gas(store)),
        ("gr_block_height", FuncsHandler::block_height(store)),
        ("gr_block_timestamp", FuncsHandler::block_timestamp(store)),
        ("gr_create_program_wgas", FuncsHandler::create_program_wgas(store, memory)),
        ("gr_exit_code", FuncsHandler::exit_code(store)),
        ("gr_gas_available", FuncsHandler::gas_available(store)),
        ("gr_debug", FuncsHandler::debug(store, memory)),
        ("gr_exit", FuncsHandler::exit(store, memory)),
        ("gr_origin", FuncsHandler::origin(store, memory)),
        ("gr_msg_id", FuncsHandler::msg_id(store, memory)),
        ("gr_program_id", FuncsHandler::program_id(store, memory)),
        ("gr_read", FuncsHandler::read(store, memory)),
        ("gr_reply", FuncsHandler::reply(store, memory)),
        ("gr_reply_wgas", FuncsHandler::reply_wgas(store, memory)),
        ("gr_reply_commit", FuncsHandler::reply_commit(store, memory)),
        ("gr_reply_commit_wgas", FuncsHandler::reply_commit_wgas(store, memory)),
        ("gr_reply_push", FuncsHandler::reply_push(store, memory)),
        ("gr_reply_to", FuncsHandler::reply_to(store, memory)),
        ("gr_send_wgas", FuncsHandler::send_wgas(store, memory)),
        ("gr_send", FuncsHandler::send(store, memory)),
        ("gr_send_commit_wgas", FuncsHandler::send_commit_wgas(store, memory)),
        ("gr_send_commit", FuncsHandler::send_commit(store, memory)),
        ("gr_send_init", FuncsHandler::send_init(store, memory)),
        ("gr_send_push", FuncsHandler::send_push(store, memory)),
        ("gr_size", FuncsHandler::size(store)),
        ("gr_source", FuncsHandler::source(store, memory)),
        ("gr_value", FuncsHandler::value(store, memory)),
        ("gr_value_available", FuncsHandler::value_available(store, memory)),
        ("gr_leave", FuncsHandler::leave(store)),
        ("gr_wait", FuncsHandler::wait(store)),
        ("gr_wake", FuncsHandler::wake(store, memory)),
        ("gr_error", FuncsHandler::error(store, memory)),
    ].into()
}
