//! Module with empty ext implemented struct for tests.
use crate::costs::RuntimeCosts;
use crate::env::Ext;
use crate::ids::{MessageId, ProgramId};
use crate::memory::{Memory, WasmPageNumber};
use crate::message::{HandlePacket, IncomingMessage, InitPacket, MessageContext, ReplyPacket};
use codec::Encode;
use core::fmt;
use gear_core_errors::{CoreError, TerminationReason};

///Empty struct for alloc error handling
#[derive(Debug)]
pub struct AllocError;

impl fmt::Display for AllocError {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        unreachable!()
    }
}

impl CoreError for AllocError {
    fn from_termination_reason(_reason: TerminationReason) -> Self {
        unreachable!()
    }

    fn as_termination_reason(&self) -> Option<TerminationReason> {
        unreachable!()
    }
}

// Test function of format `Fn(&mut E: Ext) -> R`
// to call `fn with<R>(&self, f: impl FnOnce(&mut E) -> R) -> R`.
// For example, returns the field of ext's inner value.
#[allow(dead_code)]
pub(crate) fn converter(e: &mut ExtImplementedStruct) -> ProgramId {
    e.message_context.program_id()
}

/// Struct with internal value to interact with ExtCarrier
#[derive(Debug, PartialEq, Clone)]
pub struct ExtImplementedStruct {
    message_context: MessageContext,
}

/// Empty Ext implementation for test struct
impl ExtImplementedStruct {
    ///Create instance with message context
    pub fn new_(
        source: ProgramId,
        destination: ProgramId,
        message: Option<IncomingMessage>,
    ) -> Self {
        Self {
            message_context: MessageContext::new(
                message.unwrap_or_else(|| {
                    IncomingMessage::new(
                        Default::default(),
                        source,
                        Option::<bool>::encode(&None),
                        0,
                        0,
                        None,
                    )
                }),
                destination,
                None,
            ),
        }
    }

    ///Create empty instance
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self {
            message_context: MessageContext::new(
                IncomingMessage::new(
                    Default::default(),
                    Default::default(),
                    Option::<bool>::encode(&None),
                    0,
                    0,
                    None,
                ),
                Default::default(),
                None,
            ),
        }
    }
}

/// Empty Ext implementation for test struct
impl Ext for ExtImplementedStruct {
    type Error = AllocError;

    fn alloc(
        &mut self,
        _pages: WasmPageNumber,
        _mem: &mut dyn Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
        Err(AllocError)
    }
    fn block_height(&mut self) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn block_timestamp(&mut self) -> Result<u64, Self::Error> {
        Ok(0)
    }
    fn origin(&mut self) -> Result<ProgramId, Self::Error> {
        Ok(ProgramId::from(0))
    }
    fn send_init(&mut self) -> Result<usize, Self::Error> {
        Ok(0)
    }
    fn send_push(&mut self, _handle: usize, _buffer: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn send_commit(
        &mut self,
        _handle: usize,
        _msg: HandlePacket,
    ) -> Result<MessageId, Self::Error> {
        Ok(MessageId::default())
    }
    fn reply_push(&mut self, _buffer: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reply_commit(&mut self, _msg: ReplyPacket) -> Result<MessageId, Self::Error> {
        Ok(MessageId::default())
    }
    fn reply_to(&mut self) -> Result<Option<(MessageId, i32)>, Self::Error> {
        Ok(None)
    }
    fn source(&mut self) -> Result<ProgramId, Self::Error> {
        Ok(ProgramId::from(0))
    }
    fn exit(&mut self, _value_destination: ProgramId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn message_id(&mut self) -> Result<MessageId, Self::Error> {
        Ok(0.into())
    }
    fn program_id(&mut self) -> Result<ProgramId, Self::Error> {
        Ok(0.into())
    }
    fn free(&mut self, _page: WasmPageNumber) -> Result<(), Self::Error> {
        Ok(())
    }
    fn debug(&mut self, _data: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn leave(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn msg(&mut self) -> &[u8] {
        self.message_context.current().payload()
    }
    fn gas(&mut self, _amount: u32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn charge_gas(&mut self, _amount: u32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn charge_gas_runtime(&mut self, _costs: RuntimeCosts) -> Result<(), Self::Error> {
        Ok(())
    }
    fn refund_gas(&mut self, _amount: u32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn gas_available(&mut self) -> Result<u64, Self::Error> {
        Ok(1_000_000)
    }
    fn value(&mut self) -> Result<u128, Self::Error> {
        Ok(0)
    }
    fn value_available(&mut self) -> Result<u128, Self::Error> {
        Ok(1_000_000)
    }
    fn wait(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn wake(&mut self, _waker_id: MessageId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn create_program(&mut self, _packet: InitPacket) -> Result<ProgramId, Self::Error> {
        Ok(Default::default())
    }
}
