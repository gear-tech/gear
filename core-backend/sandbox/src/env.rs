// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! sp-sandbox environment for running a module.

use super::memory::MemoryWrap;
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    string::{String, ToString},
    vec,
};
use sp_sandbox::{EnvironmentDefinitionBuilder, HostError, Instance, ReturnValue, Value};

use gear_backend_common::funcs;
use gear_core::env::{Ext, LaterExt};
use gear_core::memory::{Memory, PageBuf, PageNumber};
use gear_core::message::{MessageId, OutgoingPacket, ReplyPacket};
use gear_core::program::ProgramId;

struct Runtime<E: Ext + 'static> {
    ext: LaterExt<E>,
    trap_reason: Option<&'static str>,
}

/// Environment to run one module at a time providing Ext.
pub struct SandboxEnvironment<E: Ext + 'static> {
    ext: LaterExt<E>,
}

impl<E: Ext + 'static> SandboxEnvironment<E> {
    /// New environment.
    ///
    /// To run actual function with provided external environment, `setup_and_run` should be used.
    pub fn new() -> Self {
        Self {
            ext: LaterExt::new(),
        }
    }

    /// Setup external environment and run closure.
    ///
    /// Setup external environment by providing `ext`, run nenwly initialized instance created from
    /// provided `module`, do anything inside a `func` delegate.
    ///
    /// This will also set the beginning of the memory region to the `static_area` content _after_
    /// creatig instance.
    pub(crate) fn setup_and_run_inner(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn Memory,
        entry_point: &str,
    ) -> (anyhow::Result<()>, E) {
        fn send<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let program_id_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let payload_ptr: i32 = match args[1] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let payload_len: i32 = match args[2] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let gas_limit: i64 = match args[3] {
                Value::I64(val) => val,
                _ => return Err(HostError),
            };
            let value_ptr: i32 = match args[4] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let message_id_ptr: i32 = match args[5] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };

            let result = ctx
                .ext
                .with(|ext: &mut E| -> Result<(), &'static str> {
                    let dest: ProgramId = funcs::get_id(ext, program_id_ptr).into();
                    let payload = funcs::get_vec(ext, payload_ptr, payload_len);
                    let value = funcs::get_u128(ext, value_ptr);
                    let message_id = ext.send(OutgoingPacket::new(
                        dest,
                        payload.into(),
                        gas_limit as _,
                        value,
                    ))?;
                    ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
                    Ok(())
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|_err| {
                    ctx.trap_reason = Some("Trapping: unable to send message");
                    HostError
                });
            result
        }

        fn send_commit<E: Ext>(
            ctx: &mut Runtime<E>,
            args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            let handle_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let message_id_ptr: i32 = match args[1] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let program_id_ptr: i32 = match args[2] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let gas_limit: i64 = match args[3] {
                Value::I64(val) => val,
                _ => return Err(HostError),
            };
            let value_ptr: i32 = match args[4] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };

            ctx.ext
                .with(|ext: &mut E| -> Result<(), &'static str> {
                    let dest: ProgramId = funcs::get_id(ext, program_id_ptr).into();
                    let value = funcs::get_u128(ext, value_ptr);
                    let message_id = ext.send_commit(
                        handle_ptr as _,
                        OutgoingPacket::new(dest, vec![].into(), gas_limit as _, value),
                    )?;
                    ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
                    Ok(())
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|_err| {
                    ctx.trap_reason = Some("Trapping: unable to send message");
                    HostError
                })
        }

        fn send_init<E: Ext>(
            ctx: &mut Runtime<E>,
            _args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            ctx.ext
                .with(|ext: &mut E| ext.send_init())
                .and_then(|res| res.map(|handle| ReturnValue::Value(Value::I32(handle as _))))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })
        }

        fn send_push<E: Ext>(
            ctx: &mut Runtime<E>,
            args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            let handle_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let payload_ptr: i32 = match args[1] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let payload_len: i32 = match args[2] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            ctx.ext
                .with(|ext: &mut E| {
                    let payload = funcs::get_vec(ext, payload_ptr, payload_len);
                    ext.send_push(handle_ptr as _, &payload)
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })
        }

        fn read<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let at: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let len: i32 = match args[1] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let dest: i32 = match args[2] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let at = at as u32 as usize;
            let len = len as u32 as usize;
            ctx.ext
                .with(|ext: &mut E| {
                    let msg = ext.msg().to_vec();
                    ext.set_mem(dest as _, &msg[at..(at + len)]);
                    Ok(())
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })
        }

        fn size<E: Ext>(ctx: &mut Runtime<E>, _args: &[Value]) -> Result<ReturnValue, HostError> {
            ctx.ext
                .with(|ext: &mut E| ext.msg().len() as _)
                .map(|res| Ok(ReturnValue::Value(Value::I32(res))))
                .unwrap_or(Ok(ReturnValue::Value(Value::I32(0))))
        }

        fn exit_code<E: Ext>(
            ctx: &mut Runtime<E>,
            _args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            let reply_tuple = ctx.ext.with(|ext: &mut E| ext.reply_to()).map_err(|err| {
                ctx.trap_reason = Some(err);
                HostError
            })?;

            if let Some((_, exit_code)) = reply_tuple {
                Ok(ReturnValue::Value(Value::I32(exit_code)))
            } else {
                ctx.trap_reason = Some("Trapping: exit code ran into non-reply scenario");
                Err(HostError)
            }
        }

        fn gas<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let val: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            ctx.ext
                .with(|ext: &mut E| ext.charge_gas(val as _))
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|_err| {
                    ctx.trap_reason = Some("Trapping: unable to report about gas used");
                    HostError
                })
        }

        fn alloc<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let pages: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let pages = pages as u32;

            let ptr = ctx
                .ext
                .with(|ext: &mut E| ext.alloc(pages.into()))
                .and_then(|v| {
                    v.map(|v| {
                        let ptr = v.raw();
                        log::debug!("ALLOC: {} pages at {}", pages, ptr);
                        ptr
                    })
                });
            ptr.map(|res| ReturnValue::Value(Value::I32(res as i32)))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })
        }

        fn free<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let pages: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let page = pages as u32;
            if let Err(e) = ctx
                .ext
                .with(|ext: &mut E| ext.free(page.into()))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })?
            {
                log::debug!("FREE ERROR: {:?}", e);
            } else {
                log::debug!("FREE: {}", page);
            }
            Ok(ReturnValue::Unit)
        }

        fn block_height<E: Ext>(
            ctx: &mut Runtime<E>,
            _args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            let block_height = ctx.ext.with(|ext: &mut E| ext.block_height()).unwrap_or(0);
            Ok(ReturnValue::Value(Value::I32(block_height as i32)))
        }

        fn block_timestamp<E: Ext>(
            ctx: &mut Runtime<E>,
            _args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            let block_timestamp = ctx
                .ext
                .with(|ext: &mut E| ext.block_timestamp())
                .unwrap_or(0);
            Ok(ReturnValue::Value(Value::I32(block_timestamp as i32)))
        }

        fn reply<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let payload_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let payload_len: i32 = match args[1] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let gas_limit: i64 = match args[2] {
                Value::I64(val) => val,
                _ => return Err(HostError),
            };
            let value_ptr: i32 = match args[3] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let result = ctx
                .ext
                .with(|ext: &mut E| {
                    let payload = funcs::get_vec(ext, payload_ptr, payload_len);
                    let value = funcs::get_u128(ext, value_ptr);
                    ext.reply(ReplyPacket::new(0, payload.into(), gas_limit as _, value))
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|_err| {
                    ctx.trap_reason = Some("Trapping: unable to send reply message");
                    HostError
                });
            result
        }

        fn reply_commit<E: Ext>(
            ctx: &mut Runtime<E>,
            args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            let message_id_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let gas_limit: i64 = match args[1] {
                Value::I64(val) => val,
                _ => return Err(HostError),
            };
            let value_ptr: i32 = match args[2] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            ctx.ext
                .with(|ext: &mut E| -> Result<(), &'static str> {
                    let value = funcs::get_u128(ext, value_ptr);
                    let message_id = ext.reply_commit(ReplyPacket::new(
                        0,
                        vec![].into(),
                        gas_limit as _,
                        value,
                    ))?;
                    ext.set_mem(message_id_ptr as isize as _, message_id.as_slice());
                    Ok(())
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|_err| {
                    ctx.trap_reason = Some("Trapping: unable to send message");
                    HostError
                })
        }

        fn reply_to<E: Ext>(
            ctx: &mut Runtime<E>,
            args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            let dest: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let maybe_message_id = ctx.ext.with(|ext: &mut E| ext.reply_to()).map_err(|err| {
                ctx.trap_reason = Some(err);
                HostError
            })?;

            match maybe_message_id {
                Some((message_id, _)) => ctx
                    .ext
                    .with(|ext| {
                        ext.set_mem(dest as isize as _, message_id.as_slice());
                    })
                    .map_err(|err| {
                        ctx.trap_reason = Some(err);
                        HostError
                    })?,
                None => {
                    ctx.trap_reason = Some("Not running in the reply context");
                    return Err(HostError);
                }
            };

            Ok(ReturnValue::Unit)
        }

        fn reply_push<E: Ext>(
            ctx: &mut Runtime<E>,
            args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            let payload_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            let payload_len: i32 = match args[1] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            ctx.ext
                .with(|ext: &mut E| {
                    let payload = funcs::get_vec(ext, payload_ptr, payload_len);
                    ext.reply_push(&payload)
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|_err| {
                    ctx.trap_reason = Some("Trapping: unable to push payload into reply");
                    HostError
                })
        }

        fn debug<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let str_ptr: usize = match args[0] {
                Value::I32(val) => val as u32 as usize,
                _ => return Err(HostError),
            };
            let str_len: usize = match args[1] {
                Value::I32(val) => val as u32 as usize,
                _ => return Err(HostError),
            };
            ctx.ext
                .with_fallible(|ext: &mut E| -> Result<(), &'static str> {
                    let mut data = vec![0u8; str_len];
                    ext.get_mem(str_ptr, &mut data);
                    match String::from_utf8(data) {
                        Ok(s) => ext.debug(&s),
                        Err(_) => Err("Failed to parse debug string"),
                    }
                })
                .map(|_| ReturnValue::Unit)
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })
        }

        fn gas_available<E: Ext>(
            ctx: &mut Runtime<E>,
            _args: &[Value],
        ) -> Result<ReturnValue, HostError> {
            let gas_available = ctx.ext.with(|ext: &mut E| ext.gas_available()).unwrap_or(0);
            Ok(ReturnValue::Value(Value::I64(gas_available as i64)))
        }

        fn msg_id<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let msg_id_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            ctx.ext
                .with(|ext: &mut E| {
                    let message_id = ext.message_id();
                    ext.set_mem(msg_id_ptr as isize as _, message_id.as_slice());
                    Ok(())
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })
        }

        fn source<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let source_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            ctx.ext
                .with(|ext: &mut E| {
                    let source = ext.source();
                    ext.set_mem(source_ptr as isize as _, source.as_slice());
                    Ok(())
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })
        }

        fn value<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let value_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            ctx.ext
                .with(|ext: &mut E| {
                    funcs::set_u128(ext, value_ptr, ext.value());
                    Ok(())
                })
                .and_then(|res| res.map(|_| ReturnValue::Unit))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })
        }

        fn wait<E: Ext>(ctx: &mut Runtime<E>, _args: &[Value]) -> Result<ReturnValue, HostError> {
            let _: Result<ReturnValue, HostError> = ctx
                .ext
                .with(|ext: &mut E| ext.wait())
                .map(|_| Ok(ReturnValue::Unit))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })?;
            ctx.trap_reason = Some("exit");
            Err(HostError)
        }

        fn wake<E: Ext>(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
            let waker_id_ptr: i32 = match args[0] {
                Value::I32(val) => val,
                _ => return Err(HostError),
            };
            ctx.ext
                .with(|ext: &mut E| {
                    let waker_id: MessageId = funcs::get_id(ext, waker_id_ptr).into();
                    ext.wake(waker_id)
                })
                .map(|_| Ok(ReturnValue::Unit))
                .map_err(|err| {
                    ctx.trap_reason = Some(err);
                    HostError
                })?
        }

        self.ext.set(ext);

        let mem = match memory.as_any().downcast_ref::<sp_sandbox::Memory>() {
            Some(mem) => mem,
            None => panic!("Memory is not sp_sandbox::Memory"),
        };

        let mut env_builder = EnvironmentDefinitionBuilder::new();
        env_builder.add_memory("env", "memory", mem.clone());
        env_builder.add_host_func("env", "alloc", alloc);
        env_builder.add_host_func("env", "free", free);
        env_builder.add_host_func("env", "gr_block_height", block_height);
        env_builder.add_host_func("env", "gr_block_timestamp", block_timestamp);
        env_builder.add_host_func("env", "gr_exit_code", exit_code);
        env_builder.add_host_func("env", "gr_send", send);
        env_builder.add_host_func("env", "gr_send_commit", send_commit);
        env_builder.add_host_func("env", "gr_send_init", send_init);
        env_builder.add_host_func("env", "gr_send_push", send_push);
        env_builder.add_host_func("env", "gr_read", read);
        env_builder.add_host_func("env", "gr_size", size);
        env_builder.add_host_func("env", "gr_source", source);
        env_builder.add_host_func("env", "gr_value", value);
        env_builder.add_host_func("env", "gr_reply", reply);
        env_builder.add_host_func("env", "gr_reply_commit", reply_commit);
        env_builder.add_host_func("env", "gr_reply_to", reply_to);
        env_builder.add_host_func("env", "gr_reply_push", reply_push);
        env_builder.add_host_func("env", "gr_debug", debug);
        env_builder.add_host_func("env", "gr_gas_available", gas_available);
        env_builder.add_host_func("env", "gr_msg_id", msg_id);
        env_builder.add_host_func("env", "gr_wait", wait);
        env_builder.add_host_func("env", "gr_wake", wake);
        env_builder.add_host_func("env", "gas", gas);

        let mut runtime = Runtime {
            ext: self.ext.clone(),
            trap_reason: None,
        };

        let result: anyhow::Result<(), anyhow::Error> =
            match Instance::new(binary, &env_builder, &mut runtime) {
                Ok(instance) => {
                    self.run_inner(instance, memory_pages, memory, move |mut instance| {
                        let result = instance.invoke(entry_point, &[], &mut runtime);
                        if let Err(_e) = &result {
                            if let Some(trap) = runtime.trap_reason {
                                if funcs::is_exit_trap(&trap.to_string()) {
                                    // We don't propagate a trap when exit
                                    return Ok(());
                                }
                            }
                        }
                        result.map(|_| ()).map_err(|err| {
                            if let Some(trap) = runtime.trap_reason {
                                return anyhow::format_err!("{:?}", trap);
                            } else {
                                return anyhow::format_err!("{:?}", err);
                            }
                        })
                    })
                }
                Err(err) => Err(anyhow::format_err!("{:?}", err)),
            };

        let ext = self.ext.unset();

        (result, ext)
    }

    /// Create memory inside this environment.
    pub(crate) fn create_memory_inner(&self, total_pages: u32) -> MemoryWrap {
        MemoryWrap::new(sp_sandbox::Memory::new(total_pages, None).expect("Create env memory fail"))
    }

    fn run_inner(
        &mut self,
        instance: Instance<Runtime<E>>,
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn Memory,
        func: impl FnOnce(Instance<Runtime<E>>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        // Set module memory.
        memory
            .set_pages(memory_pages)
            .map_err(|e| anyhow::anyhow!("Can't set module memory: {:?}", e))?;

        func(instance)
    }
}

impl<E: Ext + 'static> Default for SandboxEnvironment<E> {
    /// Create a default environment.
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Ext> gear_backend_common::Environment<E> for SandboxEnvironment<E> {
    fn setup_and_run(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn gear_core::memory::Memory,
        entry_point: &str,
    ) -> (anyhow::Result<()>, E) {
        self.setup_and_run_inner(ext, binary, memory_pages, memory, entry_point)
    }

    fn create_memory(&self, total_pages: u32) -> Box<dyn Memory> {
        Box::new(self.create_memory_inner(total_pages))
    }
}
