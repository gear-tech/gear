use codec::Encode;
use core::fmt;
use std::collections::BTreeSet;
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, ExtInfo, IntoExtInfo, TerminationReasonKind,
};
use gear_core::{
    costs::RuntimeCosts,
    env::Ext,
    gas::GasAmount,
    ids::{MessageId, ProgramId},
    memory::{Memory, WasmPageNumber},
    message::{HandlePacket, IncomingMessage, InitPacket, MessageContext, ReplyPacket},
};
use gear_core_errors::{CoreError, ExtError, MemoryError};

#[derive(Debug)]
pub struct AllocError;

impl fmt::Display for AllocError {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        unreachable!()
    }
}

impl CoreError for AllocError {}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ExtImplementedStruct {
    message_context: MessageContext,
}

impl AsTerminationReason for AllocError {
    fn as_termination_reason(&self) -> Option<&TerminationReasonKind> {
        unreachable!();
    }
}

impl IntoExtError for AllocError {
    fn into_ext_error(self) -> Result<ExtError, Self> {
        unreachable!();
    }
}

impl IntoExtInfo for ExtImplementedStruct {
    fn into_ext_info(self, _: &dyn Memory) -> Result<ExtInfo, (MemoryError, GasAmount)> {
        unreachable!();
    }

    fn into_gas_amount(self) -> GasAmount {
        unreachable!();
    }

    fn last_error(&self) -> Option<&ExtError> {
        unreachable!()
    }
}

impl ExtImplementedStruct {
    #[allow(dead_code)]
    pub fn new(
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
}

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
    fn forbidden_funcs(&self) -> &BTreeSet<&'static str> { unreachable!() }
}
