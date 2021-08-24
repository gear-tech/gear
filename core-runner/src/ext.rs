use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasCounter},
    memory::{MemoryContext, PageNumber},
    message::{ExitCode, MessageContext, MessageId, OutgoingPacket, ReplyPacket},
    program::ProgramId,
};

use crate::util::BlakeMessageIdGenerator;

use alloc::boxed::Box;

pub struct Ext {
    pub memory_context: MemoryContext,
    pub messages: MessageContext<BlakeMessageIdGenerator>,
    pub gas_counter: Box<dyn GasCounter>,
    pub alloc_cost: u64,
    pub last_error_returned: Option<&'static str>,
}

impl Ext {
    fn return_with_tracing<T>(
        &mut self,
        result: Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        match result {
            Ok(result) => Ok(result),
            Err(error_string) => {
                self.last_error_returned = Some(error_string);
                Err(error_string)
            }
        }
    }
}

impl EnvExt for Ext {
    fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, &'static str> {
        self.gas(pages.raw() * self.alloc_cost as u32)?;

        let result = self
            .memory_context
            .alloc(pages)
            .map_err(|_e| "Allocation error");

        self.return_with_tracing(result)
    }

    fn send(&mut self, msg: OutgoingPacket) -> Result<MessageId, &'static str> {
        if self.gas_counter.reduce(msg.gas_limit()) != ChargeResult::Enough {
            return self
                .return_with_tracing(Err("Gas limit exceeded while trying to send message"));
        }

        let result = self.messages.send(msg).map_err(|_e| "Message send error");

        self.return_with_tracing(result)
    }

    fn send_init(&mut self) -> Result<usize, &'static str> {
        let result = self.messages.send_init().map_err(|_e| "Message init error");

        self.return_with_tracing(result)
    }

    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), &'static str> {
        let result = self
            .messages
            .send_push(handle, buffer)
            .map_err(|_e| "Payload push error");

        self.return_with_tracing(result)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), &'static str> {
        let result = self
            .messages
            .reply_push(buffer)
            .map_err(|_e| "Reply payload push error");

        self.return_with_tracing(result)
    }

    fn send_commit(
        &mut self,
        handle: usize,
        msg: OutgoingPacket,
    ) -> Result<MessageId, &'static str> {
        if self.gas_counter.reduce(msg.gas_limit()) != ChargeResult::Enough {
            return self
                .return_with_tracing(Err("Gas limit exceeded while trying to send message"));
        };

        let result = self
            .messages
            .send_commit(handle, msg)
            .map_err(|_e| "Message commit error");

        self.return_with_tracing(result)
    }

    fn reply(&mut self, msg: ReplyPacket) -> Result<(), &'static str> {
        let result = self.messages.reply(msg).map_err(|_e| "Reply error");

        self.return_with_tracing(result)
    }

    fn reply_to(&self) -> Option<(MessageId, ExitCode)> {
        self.messages.current().reply()
    }

    fn source(&mut self) -> ProgramId {
        self.messages.current().source()
    }

    fn message_id(&mut self) -> MessageId {
        self.messages.current().id()
    }

    fn free(&mut self, ptr: PageNumber) -> Result<(), &'static str> {
        let result = self.memory_context.free(ptr).map_err(|_e| "Free error");

        self.return_with_tracing(result)
    }

    fn debug(&mut self, data: &str) -> Result<(), &'static str> {
        log::debug!("DEBUG: {}", data);

        Ok(())
    }

    fn set_mem(&mut self, ptr: usize, val: &[u8]) {
        self.memory_context
            .memory()
            .write(ptr, val)
            // TODO: remove and propagate error, issue #97
            .expect("Memory out of bounds.");
    }

    fn get_mem(&self, ptr: usize, buffer: &mut [u8]) {
        self.memory_context.memory().read(ptr, buffer);
    }

    fn msg(&mut self) -> &[u8] {
        self.messages.current().payload()
    }

    fn gas(&mut self, val: u32) -> Result<(), &'static str> {
        if self.gas_counter.charge(val as u64) == ChargeResult::Enough {
            Ok(())
        } else {
            self.return_with_tracing(Err("Gas limit exceeded"))
        }
    }

    fn gas_available(&mut self) -> u64 {
        self.gas_counter.left()
    }

    fn value(&self) -> u128 {
        self.messages.current().value()
    }

    fn wait(&mut self) -> Result<(), &'static str> {
        let result = self
            .messages
            .wait()
            .map_err(|_| "Unable to add the message to the wait list");

        self.return_with_tracing(result)
    }

    fn wake(&mut self, waker_id: MessageId) -> Result<(), &'static str> {
        let result = self
            .messages
            .wake(waker_id)
            .map_err(|_| "Unable to mark the message to be woken");

        self.return_with_tracing(result)
    }
}
