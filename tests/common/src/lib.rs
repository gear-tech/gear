use codec::{Decode, Encode};

use gear_core::message::MessageId;
use gear_core::storage::{InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList, Storage};
use gear_core_runner::{ExtMessage, InitializeProgramInfo, MessageDispatch, Runner};

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
