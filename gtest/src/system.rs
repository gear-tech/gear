use crate::{
    log::RunResult,
    manager::ExtManager,
    program::{Program, ProgramIdWrapper},
    CoreLog, Log,
};
use colored::Colorize;
use env_logger::{Builder, Env};
use gear_core::{
    message::{Message, MessageId, Payload},
    program::ProgramId,
};
use std::{cell::RefCell, io::Write, thread};

#[derive(Debug)]
pub struct System(pub(crate) RefCell<ExtManager>);

impl Default for System {
    fn default() -> Self {
        Self(RefCell::new(ExtManager::new()))
    }
}

impl System {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init_logger(&self) {
        let _ = Builder::from_env(Env::default().default_filter_or("gwasm=debug"))
            .format(|buf, record| {
                let lvl = record.level().to_string().to_uppercase();
                let target = record.target().to_string();
                let mut msg = record.args().to_string();

                if target == "gwasm" {
                    msg = msg.replacen("DEBUG: ", "", 1);

                    writeln!(
                        buf,
                        "[{} {}] {}",
                        lvl.blue(),
                        thread::current().name().unwrap_or("unknown").white(),
                        msg.white()
                    )
                } else {
                    writeln!(
                        buf,
                        "[{} {}] {}",
                        target.red(),
                        thread::current().name().unwrap_or("unknown").white(),
                        msg.white()
                    )
                }
            })
            .format_target(false)
            .format_timestamp(None)
            .try_init();
    }

    pub fn send_message(&self, message: Message) -> RunResult {
        self.0.borrow_mut().run_message(message)
    }

    pub fn spend_blocks(&self, amount: u32) {
        self.0.borrow_mut().block_info.height += amount;
        self.0.borrow_mut().block_info.timestamp += amount as u64;
    }

    pub fn get_program<ID: Into<ProgramIdWrapper>>(&'_ self, id: ID) -> Program<'_> {
        Program {
            manager: &self.0,
            id: id.into().0,
        }
    }

    pub fn get_mailbox(&self, program_id: &ProgramId) -> Mailbox {
        Mailbox::new(self.0.borrow_mut().get_mailbox(program_id), self)
    }

    pub(crate) fn fetch_inc_message_nonce(&self) -> u64 {
        self.0.borrow_mut().fetch_inc_message_nonce()
    }
}

#[derive(Debug)]
pub struct Mailbox<'system_lifetime> {
    mail: Vec<Message>,
    system_reference: &'system_lifetime System,
}

impl<'system_lifetime> Mailbox<'system_lifetime> {
    pub fn new(
        messages: Vec<Message>,
        system_reference: &'system_lifetime System,
    ) -> Mailbox<'system_lifetime> {
        Mailbox {
            mail: messages,
            system_reference,
        }
    }

    pub fn take_message(&mut self, log: Log) -> Option<MessageReplier> {
        for index in 0..self.mail.len() {
            if log.eq(&self.mail[index]) {
                let message = self.mail.remove(index);
                return Some(MessageReplier::new(message, self.system_reference));
            }
        }
        None
    }
}

pub struct MessageReplier<'system_lifetime> {
    log: CoreLog,
    system_reference: &'system_lifetime System,
}

impl<'system_lifetime> MessageReplier<'system_lifetime> {
    pub fn new(
        message: Message,
        system: &'system_lifetime System,
    ) -> MessageReplier<'system_lifetime> {
        MessageReplier {
            log: CoreLog::from_message(message),
            system_reference: system,
        }
    }

    pub(crate) fn reply(&self, payload: Payload, value: u128) -> Option<RunResult> {
        let message = self.log.generate_reply(
            payload,
            MessageId::from(self.system_reference.fetch_inc_message_nonce()),
            value,
        );
        let old_payload = self.log.get_payload();
        let old_message = self
            .system_reference
            .0
            .borrow_mut()
            .take_message(&message.source, &old_payload);
        if old_message.is_some() {
            return Some(self.system_reference.send_message(message));
        }
        None
    }

    pub fn reply_bytes(&self, raw_payload: &[u8], value: u128) -> Option<RunResult> {
        self.reply(raw_payload.to_vec().into(), value)
    }
}
