use alloc::string::String;
use gear_core::env::{Ext, LaterExt};
use gear_core::message::{OutgoingPacket, ReplyPacket};
use gear_core::program::ProgramId;

pub(crate) fn alloc<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<u32, &'static str> {
    move |pages: i32| {
        let pages = pages as u32;

        let ptr = ext
            .with(|ext: &mut E| ext.alloc(pages.into()))
            .map(|v| {
                let ptr = v.raw();
                log::debug!("ALLOC: {} pages at {}", pages, ptr);
                ptr
            })
            .unwrap_or(0u32);

        Ok(ptr)
    }
}

pub(crate) fn free<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |page: i32| {
        let page = page as u32;
        if let Err(e) = ext.with(|ext: &mut E| ext.free(page.into())) {
            log::debug!("FREE ERROR: {:?}", e);
        } else {
            log::debug!("FREE: {}", page);
        }
        Ok(())
    }
}

pub(crate) fn charge<E: Ext>(ext: LaterExt<E>) -> impl Fn(i64) -> Result<(), &'static str> {
    move |gas: i64| {
        if ext.with(|ext: &mut E| ext.charge(gas as u64)).is_err() {
            Err("Trapping: unable to charge gas for reserve")
        } else {
            Ok(())
        }
    }
}

pub(crate) fn commit<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |handle_ptr: i32| {
        let handle_ptr = handle_ptr as u32 as usize;

        let result = ext.with(|ext: &mut E| ext.commit(handle_ptr));
        if result.is_err() {
            return Err("Trapping: unable to commit and send message");
        }

        Ok(())
    }
}

pub(crate) fn debug<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32, i32) -> Result<(), &'static str> {
    move |str_ptr: i32, str_len: i32| {
        let str_ptr = str_ptr as u32 as usize;
        let str_len = str_len as u32 as usize;
        ext.with(|ext: &mut E| {
            let mut data = vec![0u8; str_len];
            ext.get_mem(str_ptr, &mut data);
            let debug_str = unsafe { String::from_utf8_unchecked(data) };
            log::debug!("DEBUG: {}", debug_str);
        });
        Ok(())
    }
}

pub(crate) fn gas<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |val: i32| {
        if ext.with(|ext: &mut E| ext.gas(val as _)).is_err() {
            Err("Trapping: unable to report about gas used")
        } else {
            Ok(())
        }
    }
}

pub(crate) fn init<E: Ext>(
    ext: LaterExt<E>,
) -> impl Fn(i32, i32, i32, i64, i32) -> Result<i32, &'static str> {
    move |program_id_ptr: i32,
          message_ptr: i32,
          message_len: i32,
          gas_limit: i64,
          value_ptr: i32| {
        let message_ptr = message_ptr as u32 as usize;
        let message_len = message_len as u32 as usize;
        let result = ext.with(|ext: &mut E| {
            let mut data = vec![0u8; message_len];
            ext.get_mem(message_ptr, &mut data);
            let mut program_id = [0u8; 32];
            ext.get_mem(program_id_ptr as isize as _, &mut program_id);
            let program_id = ProgramId::from_slice(&program_id);

            let mut value_le = [0u8; 16];
            ext.get_mem(value_ptr as isize as _, &mut value_le);

            ext.init(OutgoingPacket::new(
                program_id,
                data.into(),
                gas_limit as _,
                u128::from_le_bytes(value_le),
            ))
        });

        if result.is_err() {
            return Err("Trapping: unable to init message");
        };

        Ok(result.unwrap() as isize as i32)
    }
}

pub(crate) fn msg_id<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |msg_id_ptr: i32| {
        ext.with(|ext: &mut E| {
            let message_id = ext.message_id();
            ext.set_mem(msg_id_ptr as isize as _, message_id.as_slice());
        });
        Ok(())
    }
}

pub(crate) fn push<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32, i32, i32) -> Result<(), &'static str> {
    move |handle_ptr: i32, message_ptr: i32, message_len: i32| {
        let handle_ptr = handle_ptr as u32 as usize;
        let message_ptr = message_ptr as u32 as usize;
        let message_len = message_len as u32 as usize;

        let result = ext.with(|ext: &mut E| {
            let mut data = vec![0u8; message_len];
            ext.get_mem(message_ptr, &mut data);

            ext.push(handle_ptr, &mut data)
        });

        if result.is_err() {
            return Err("Trapping: unable to push payload into message");
        }

        Ok(())
    }
}

pub(crate) fn push_reply<E: Ext>(
    ext: LaterExt<E>,
) -> impl Fn(i32, i32) -> Result<(), &'static str> {
    move |message_ptr: i32, message_len: i32| {
        let message_ptr = message_ptr as u32 as usize;
        let message_len = message_len as u32 as usize;

        let result = ext.with(|ext: &mut E| {
            let mut data = vec![0u8; message_len];
            ext.get_mem(message_ptr, &mut data);

            ext.push_reply(&mut data)
        });

        if result.is_err() {
            return Err("Trapping: unable to push payload into reply");
        }

        Ok(())
    }
}

pub(crate) fn read<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32, i32, i32) -> Result<(), &'static str> {
    move |at: i32, len: i32, dest: i32| {
        let at = at as u32 as usize;
        let len = len as u32 as usize;
        let dest = dest as u32 as usize;
        ext.with(|ext: &mut E| {
            let msg = ext.msg().to_vec();
            ext.set_mem(dest, &msg[at..at + len]);
        });
        Ok(())
    }
}

pub(crate) fn reply<E: Ext>(
    ext: LaterExt<E>,
) -> impl Fn(i32, i32, i64, i32) -> Result<(), &'static str> {
    move |message_ptr: i32, message_len: i32, gas_limit: i64, value_ptr: i32| {
        let message_ptr = message_ptr as u32 as usize;
        let message_len = message_len as u32 as usize;
        let result = ext.with(|ext: &mut E| {
            let mut data = vec![0u8; message_len];
            ext.get_mem(message_ptr, &mut data);

            let mut value_le = [0u8; 16];
            ext.get_mem(value_ptr as isize as _, &mut value_le);

            ext.reply(ReplyPacket::new(
                data.into(),
                gas_limit as _,
                u128::from_le_bytes(value_le),
            ))
        });

        if result.is_err() {
            return Err("Trapping: unable to send message");
        }

        Ok(())
    }
}

pub(crate) fn send<E: Ext>(
    ext: LaterExt<E>,
) -> impl Fn(i32, i32, i32, i64, i32) -> Result<(), &'static str> {
    move |program_id_ptr: i32,
          message_ptr: i32,
          message_len: i32,
          gas_limit: i64,
          value_ptr: i32| {
        let message_ptr = message_ptr as u32 as usize;
        let message_len = message_len as u32 as usize;
        let result = ext.with(|ext: &mut E| {
            let mut data = vec![0u8; message_len];
            ext.get_mem(message_ptr, &mut data);
            let mut program_id = [0u8; 32];
            ext.get_mem(program_id_ptr as isize as _, &mut program_id);
            let program_id = ProgramId::from_slice(&program_id);

            let mut value_le = [0u8; 16];
            ext.get_mem(value_ptr as isize as _, &mut value_le);

            ext.send(OutgoingPacket::new(
                program_id,
                data.into(),
                gas_limit as _,
                u128::from_le_bytes(value_le),
            ))
        });

        if result.is_err() {
            return Err("Trapping: unable to send message");
        }

        Ok(())
    }
}

pub(crate) fn size<E: Ext>(ext: LaterExt<E>) -> impl Fn() -> i32 {
    move || ext.with(|ext: &mut E| ext.msg().len() as isize as i32)
}

pub(crate) fn source<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |source_ptr: i32| {
        ext.with(|ext: &mut E| {
            let source = ext.source();
            ext.set_mem(source_ptr as isize as _, source.as_slice());
        });
        Ok(())
    }
}

pub(crate) fn value<E: Ext>(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), &'static str> {
    move |value_ptr: i32| {
        ext.with(|ext: &mut E| {
            let source = ext.value();
            ext.set_mem(value_ptr as isize as _, &source.to_le_bytes()[..]);
        });
        Ok(())
    }
}
