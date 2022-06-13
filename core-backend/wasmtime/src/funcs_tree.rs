use crate::{env::StoreData, funcs::FuncsHandler};
use alloc::collections::{BTreeMap, BTreeSet};
use gear_backend_common::{error_processor::IntoExtError, AsTerminationReason, IntoExtInfo};
use gear_core::env::Ext;
use wasmtime::{Func, Memory, Store};

pub fn build<'a, E>(
    store: &'a mut Store<StoreData<E>>,
    memory: Memory,
    forbidden_funcs: BTreeSet<&'a str>,
) -> BTreeMap<&'a str, Func>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    let mut funcs = BTreeMap::new();
    funcs.insert("alloc", FuncsHandler::alloc(store, memory));
    funcs.insert("free", FuncsHandler::free(store));
    funcs.insert("gas", FuncsHandler::gas(store));
    funcs.insert("gr_block_height", FuncsHandler::block_height(store));
    funcs.insert("gr_block_timestamp", FuncsHandler::block_timestamp(store));
    funcs.insert(
        "gr_create_program",
        FuncsHandler::create_program(store, memory),
    );
    funcs.insert(
        "gr_create_program_wgas",
        FuncsHandler::create_program_wgas(store, memory),
    );
    funcs.insert("gr_exit_code", FuncsHandler::exit_code(store));
    funcs.insert("gr_gas_available", FuncsHandler::gas_available(store));
    funcs.insert("gr_debug", FuncsHandler::debug(store, memory));
    funcs.insert("gr_exit", FuncsHandler::exit(store, memory));
    funcs.insert("gr_origin", FuncsHandler::origin(store, memory));
    funcs.insert("gr_msg_id", FuncsHandler::msg_id(store, memory));
    funcs.insert("gr_program_id", FuncsHandler::program_id(store, memory));
    funcs.insert("gr_read", FuncsHandler::read(store, memory));
    funcs.insert("gr_reply", FuncsHandler::reply(store, memory));
    funcs.insert("gr_reply_wgas", FuncsHandler::reply_wgas(store, memory));
    funcs.insert("gr_reply_commit", FuncsHandler::reply_commit(store, memory));
    funcs.insert(
        "gr_reply_commit_wgas",
        FuncsHandler::reply_commit_wgas(store, memory),
    );
    funcs.insert("gr_reply_push", FuncsHandler::reply_push(store, memory));
    funcs.insert("gr_reply_to", FuncsHandler::reply_to(store, memory));
    funcs.insert("gr_send_wgas", FuncsHandler::send_wgas(store, memory));
    funcs.insert("gr_send", FuncsHandler::send(store, memory));
    funcs.insert(
        "gr_send_commit_wgas",
        FuncsHandler::send_commit_wgas(store, memory),
    );
    funcs.insert("gr_send_commit", FuncsHandler::send_commit(store, memory));
    funcs.insert("gr_send_init", FuncsHandler::send_init(store, memory));
    funcs.insert("gr_send_push", FuncsHandler::send_push(store, memory));
    funcs.insert("gr_size", FuncsHandler::size(store));
    funcs.insert("gr_source", FuncsHandler::source(store, memory));
    funcs.insert("gr_value", FuncsHandler::value(store, memory));
    funcs.insert(
        "gr_value_available",
        FuncsHandler::value_available(store, memory),
    );
    funcs.insert("gr_leave", FuncsHandler::leave(store));
    funcs.insert("gr_wait", FuncsHandler::wait(store));
    funcs.insert("gr_wake", FuncsHandler::wake(store, memory));
    funcs.insert("gr_error", FuncsHandler::error(store, memory));

    forbidden_funcs.iter().for_each(|func_name| {
        funcs.insert(*func_name, FuncsHandler::forbidden(store));
    });

    funcs
}
