#![allow(unused)]

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use gear_core::{env::Ext as EnvExt, gas::*, memory::*, message::*, program::*, storage::*};

use gear_backend_common::Environment;

use crate::configs::{BlockInfo, ExecutionSettings};
use crate::ext::Ext;
use crate::id;

#[derive(Clone)]
enum DispatchKind {
    Init,
    Handle,
    HandleReply,
}

impl DispatchKind {
    fn into_entry(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Handle => "handle",
            Self::HandleReply => "handle_reply",
        }
    }
}

#[derive(Clone)]
struct Dispatch {
    kind: DispatchKind,
    message: Message,
}

enum DispatchResultKind {
    Success,
    Trap(Option<&'static str>),
    Wait,
}

struct DispatchResult {
    kind: DispatchResultKind,

    program: Program,
    dispatch: Dispatch,

    outgoing: Vec<Message>,
    awakening: Vec<MessageId>,

    gas_left: u64,
    gas_burned: u64,

    page_update: BTreeMap<PageNumber, Vec<u8>>,
}

const ERR_EXIT_CODE: i32 = 1;

impl DispatchResult {
    fn program(self) -> Program {
        self.program
    }

    fn program_id(&self) -> ProgramId {
        self.program.id()
    }

    fn message_id(&self) -> MessageId {
        self.dispatch.message.id()
    }

    fn dispatch(&self) -> Dispatch {
        self.dispatch.clone()
    }

    fn gas_left(&self) -> u64 {
        self.gas_left
    }

    fn gas_burned(&self) -> u64 {
        self.gas_burned
    }

    fn outgoing(&self) -> Vec<Message> {
        self.outgoing.clone()
    }

    fn awakening(&self) -> Vec<MessageId> {
        self.awakening.clone()
    }

    fn page_update(&self) -> BTreeMap<PageNumber, Vec<u8>> {
        self.page_update.clone()
    }

    fn trap_reply(&mut self) -> Option<Message> {
        if let Some((_, exit_code)) = self.dispatch.message.reply() {
            if exit_code != 0 {
                return None;
            }
        };

        Some(Message::new_reply(
            id::next_message_id(&mut self.program),
            self.program_id(),
            self.dispatch.message.source(),
            Default::default(),
            0,
            0,
            self.message_id(),
            ERR_EXIT_CODE,
        ))
    }
}

enum JournalNote {
    SendMessage {
        origin: MessageId,
        message: Message,
    },
    ExecutionFail {
        origin: MessageId,
        program_id: ProgramId,
        reason: &'static str,
    },
    WaitDispatch(Dispatch),
    MessageConsumed(MessageId),
    NotProcessed(Vec<Dispatch>),
    GasBurned {
        origin: MessageId,
        amount: u64,
    },
    WakeMessage {
        origin: MessageId,
        message_id: MessageId,
    },
    UpdatePage {
        origin: MessageId,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Vec<u8>,
    },
}

trait ResourceLimiter {
    fn can_process(&self, dispatch: &Dispatch) -> bool;
    fn pay_for(&mut self, dispatch: &Dispatch);
}

fn execute_wasm<E: Environment<Ext>>(
    program: Program,
    dispatch: Dispatch,
    settings: ExecutionSettings,
) -> Result<DispatchResult, ExecutionError> {
    let mut env = E::new();

    let Dispatch { kind, message } = dispatch;

    // env calls and gas consuming

    Err(ExecutionError {
        program,
        reason: "",
    })
}

struct ProcessResult {
    program: Program,
    journal: Vec<JournalNote>,
}

struct ExecutionError {
    program: Program,
    reason: &'static str,
}

fn process<E: Environment<Ext>>(
    program: Program,
    dispatch: Dispatch,
    block_info: BlockInfo,
) -> ProcessResult {
    let mut journal = Vec::new();

    let execution_settings = ExecutionSettings::new(block_info);

    let origin = dispatch.message.id();

    let mut dispatch_result = match execute_wasm::<E>(program, dispatch, execution_settings) {
        Ok(res) => res,
        Err(e) => {
            return ProcessResult {
                journal: vec![JournalNote::ExecutionFail {
                    origin,
                    program_id: e.program.id(),
                    reason: e.reason,
                }],
                program: e.program,
            }
        }
    };

    journal.push(JournalNote::GasBurned {
        origin,
        amount: dispatch_result.gas_burned(),
    });

    for (page_number, data) in dispatch_result.page_update() {
        journal.push(JournalNote::UpdatePage {
            origin,
            program_id: dispatch_result.program_id(),
            page_number,
            data,
        })
    }

    for message in dispatch_result.outgoing() {
        journal.push(JournalNote::SendMessage { origin, message });
    }

    for message_id in dispatch_result.awakening() {
        journal.push(JournalNote::WakeMessage { origin, message_id });
    }

    match dispatch_result.kind {
        DispatchResultKind::Success => journal.push(JournalNote::MessageConsumed(origin)),
        DispatchResultKind::Trap(_) => {
            if let Some(message) = dispatch_result.trap_reply() {
                journal.push(JournalNote::SendMessage { origin, message })
            }

            journal.push(JournalNote::MessageConsumed(origin))
        }
        DispatchResultKind::Wait => {
            journal.push(JournalNote::WaitDispatch(dispatch_result.dispatch()))
        }
    }

    let program = dispatch_result.program();

    ProcessResult { program, journal }
}

fn process_many<E: Environment<Ext>>(
    mut programs: BTreeMap<ProgramId, Program>,
    dispatches: Vec<Dispatch>,
    resource_limiter: &mut dyn ResourceLimiter,
    block_info: BlockInfo,
) -> Vec<JournalNote> {
    let mut dispatches = dispatches.into_iter();
    let mut not_processed = Vec::new();
    let mut process_journal = Vec::new();

    for dispatch in dispatches.by_ref() {
        if !resource_limiter.can_process(&dispatch) {
            not_processed.push(dispatch);
            break;
        }

        resource_limiter.pay_for(&dispatch);

        let program = programs
            .remove(&dispatch.message.dest())
            .expect("Program wasn't found in programs");

        let ProcessResult {
            mut program,
            journal,
        } = process::<E>(program, dispatch, block_info);

        for note in &journal {
            if let JournalNote::UpdatePage {
                origin,
                program_id,
                page_number,
                data,
            } = note
            {
                program.set_page(*page_number, data).expect("Can't fail");
            }
        }

        programs.insert(program.id(), program);

        process_journal.extend(journal);
    }

    not_processed.extend(dispatches);
    process_journal.push(JournalNote::NotProcessed(not_processed));

    process_journal
}
