use codec::{Decode, Encode};

use gear_core::storage::{
    InMemoryMessageQueue, InMemoryProgramStorage, InMemoryStorage, InMemoryWaitList, Storage,
};
use gear_core::{message::MessageId, program::ProgramId};
use gear_core_runner::{Config, ExtMessage, InitializeProgramInfo, MessageDispatch, Runner};

pub type MemoryRunner = Runner<InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList>;

pub fn do_requests_in_order<Req: Encode, Rep: Decode>(
    mut runner: MemoryRunner,
    code: Vec<u8>,
    requests: Vec<Req>,
) -> Vec<Rep> {
    runner
        .init_program(InitializeProgramInfo {
            new_program_id: 1.into(),
            source_id: 0.into(),
            code,
            message: ExtMessage {
                id: 1000001.into(),
                payload: "init".as_bytes().to_vec(),
                gas_limit: u64::MAX,
                value: 0,
            },
        })
        .expect("failed to init program");

    let mut nonce = 0;

    let mut data: Vec<(u64, MessageId, Option<Rep>)> = Vec::new();

    for request in requests {
        let message_id: MessageId = (nonce + 1000002).into();
        data.push((nonce, message_id, None));
        runner.queue_message(MessageDispatch {
            source_id: 0.into(),
            destination_id: 1.into(),
            data: ExtMessage {
                id: message_id,
                gas_limit: u64::MAX,
                value: 0,
                payload: request.encode(),
            },
        });
        nonce += 1;
    }

    while runner.run_next(u64::MAX).handled != 0 {}

    let Storage { message_queue, .. } = runner.complete();

    assert_eq!(
        message_queue.log().first().map(|m| m.payload().to_vec()),
        Some(b"CREATED".to_vec())
    );

    for message in message_queue.log().iter() {
        for (_, search_message_id, ref mut reply) in data.iter_mut() {
            if message
                .reply
                .map(|(msg_id, _)| msg_id == *search_message_id)
                .unwrap_or(false)
            {
                *reply = Some(
                    Rep::decode(&mut message.payload.as_ref()).expect("Failed to decode reply"),
                );
            }
        }
    }

    data.into_iter()
        .map(|(_, _, reply)| reply.expect("No reply for message"))
        .collect()
}

pub struct MessageData<P: Encode> {
    pub id: MessageId,
    pub payload: P,
    pub gas_limit: u64,
    pub value: u128,
}

pub struct InitProgramData<P: Encode> {
    pub new_program_id: ProgramId,
    pub source_id: ProgramId,
    pub code: Vec<u8>,
    pub message: MessageData<P>,
}

pub fn do_init<P: Encode>(mut runner: MemoryRunner, init_data: InitProgramData<P>) -> MemoryRunner {
    runner
        .init_program(InitializeProgramInfo {
            new_program_id: init_data.new_program_id,
            source_id: init_data.source_id,
            code: init_data.code,
            message: ExtMessage {
                id: init_data.message.id,
                payload: init_data.message.payload.encode(),
                gas_limit: init_data.message.gas_limit,
                value: init_data.message.value,
            },
        })
        .expect("failed to init program");

    runner
}

pub struct MessageDispatchData<P: Encode> {
    pub source_id: ProgramId,
    pub destination_id: ProgramId,
    pub data: MessageData<P>,
}

pub fn do_reqrep<Req: Encode, Rep: Decode>(
    mut runner: MemoryRunner,
    message: MessageDispatchData<Req>,
) -> (MemoryRunner, Option<Rep>) {
    let message_id = message.data.id;
    runner.queue_message(MessageDispatch {
        source_id: message.source_id,
        destination_id: message.destination_id,
        data: ExtMessage {
            id: message_id,
            gas_limit: message.data.gas_limit,
            value: message.data.value,
            payload: message.data.payload.encode(),
        },
    });

    while runner.run_next(u64::MAX).handled > 0 {}

    let Storage {
        message_queue,
        program_storage,
        wait_list,
    } = runner.complete();

    let mut reply: Option<Rep> = None;

    for message in message_queue.log().iter() {
        if message
            .reply
            .map(|(msg_id, _)| msg_id == message_id)
            .unwrap_or(false)
        {
            reply =
                Some(Rep::decode(&mut message.payload.as_ref()).expect("Failed to decode reply"));
        }
    }

    (
        Runner::new(
            &Config::default(),
            InMemoryStorage {
                program_storage,
                message_queue,
                wait_list,
            },
        ),
        reply,
    )
}
