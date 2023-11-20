use gstd::ActorId;
use parity_scale_codec::{Decode, Encode};

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Request {
    Receive(u64),
    Join(ActorId),
    Report,
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Reply {
    Success,
    Failure,
    StateFailure,
    Amount(u64),
}

#[cfg(not(feature = "std"))]
pub mod wasm {
    use super::*;
    use crate::Program;
    use gstd::{
        any::Any, collections::BTreeSet, debug, future::Future, msg, prelude::*, sync::Mutex,
        ActorId,
    };

    #[derive(Eq, Ord, PartialEq, PartialOrd)]
    struct ProgramHandle {
        handle: ActorId,
    }

    impl ProgramHandle {
        fn new(handle: impl Into<ActorId>) -> Self {
            Self {
                handle: handle.into(),
            }
        }

        fn do_request<Req: Encode, Rep: Decode>(
            &self,
            request: Req,
        ) -> impl Future<Output = Result<Rep, &'static str>> {
            let encoded_request: Vec<u8> = request.encode();

            let program_handle = self.handle;
            async move {
                let reply_bytes =
                    msg::send_bytes_for_reply(program_handle, &encoded_request[..], 0, 0)
                        .expect("Error in message sending")
                        .await
                        .expect("Error in async message processing");

                Rep::decode(&mut &reply_bytes[..]).map_err(|_| "Failed to decode reply")
            }
        }

        async fn do_send(&self, amount: u64) -> Result<(), &'static str> {
            match self.do_request(Request::Receive(amount)).await? {
                Reply::Success => Ok(()),
                _ => Err("Unexpected send reply"),
            }
        }

        async fn do_report(&self) -> Result<u64, &'static str> {
            match self.do_request(Request::Report).await? {
                Reply::Amount(amount) => Ok(amount),
                _ => Err("Unexpected send reply"),
            }
        }
    }

    #[derive(Default)]
    pub(crate) struct Distributor {
        nodes: Mutex<BTreeSet<ProgramHandle>>,
        amount: u64,
    }

    impl Program for Distributor {
        fn init(_: Box<dyn Any>) -> Self {
            msg::reply((), 0).unwrap();
            Self::default()
        }

        fn handle(&'static mut self) {
            gstd::message_loop(self.handle_request());
        }
    }

    impl Distributor {
        async fn handle_request(&mut self) {
            let reply = match msg::load::<Request>() {
                Ok(request) => match request {
                    Request::Receive(amount) => self.handle_receive(amount).await,
                    Request::Join(program_id) => self.handle_join(program_id).await,
                    Request::Report => self.handle_report().await,
                },
                Err(e) => {
                    debug!("Error processing request: {e:?}");
                    Reply::Failure
                }
            };

            debug!("Handle request finished");
            msg::reply(reply, 0).unwrap();
        }

        async fn handle_receive(&mut self, amount: u64) -> Reply {
            debug!("Handling receive {amount}");

            let nodes = self.nodes.lock().await;
            let subnodes_count = nodes.as_ref().len() as u64;

            if subnodes_count > 0 {
                let distributed_per_node = amount / subnodes_count;
                let distributed_total = distributed_per_node * subnodes_count;
                let mut left_over = amount - distributed_total;

                if distributed_per_node > 0 {
                    for program in nodes.as_ref().iter() {
                        if program.do_send(distributed_per_node).await.is_err() {
                            // reclaiming amount from nodes that fail!
                            left_over += distributed_per_node;
                        }
                    }
                }

                debug!("Set own amount to: {left_over}");
                self.amount += left_over;
            } else {
                debug!("Set own amount to: {amount}");
                self.amount += amount;
            }

            Reply::Success
        }

        async fn handle_join(&mut self, program_id: ActorId) -> Reply {
            let mut nodes = self.nodes.lock().await;
            debug!("Inserting into nodes");
            nodes.as_mut().insert(ProgramHandle::new(program_id));
            Reply::Success
        }

        async fn handle_report(&mut self) -> Reply {
            debug!("Own amount {}", self.amount);

            let nodes = self.nodes.lock().await;

            for program in nodes.as_ref().iter() {
                debug!("Querying next node");
                self.amount += match program.do_report().await {
                    Ok(amount) => {
                        debug!("Sub-node result: {amount}");
                        amount
                    }
                    Err(_) => {
                        // skipping erroneous sub-nodes!
                        debug!("Skipping erroneous node");
                        0
                    }
                }
            }

            Reply::Amount(self.amount)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::InitMessage;

    use super::{Reply, Request};
    use gtest::{Log, Program, System};

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = 42;

        let res = program.send(from, InitMessage::Distributor);
        let log = Log::builder().source(program.id()).dest(from);
        assert!(res.contains(&log));
    }

    #[test]
    fn single_program() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = 42;

        let _res = program.send(from, InitMessage::Distributor);

        let res = program.send(from, Request::Receive(10));
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        let res = program.send(from, Request::Report);
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Amount(10));
        assert!(res.contains(&log));
    }

    fn multi_program_setup(
        system: &System,
        program_1_id: u64,
        program_2_id: u64,
        program_3_id: u64,
    ) -> (Program, Program, Program) {
        system.init_logger();

        let from = 42;

        let program_1 = Program::current_with_id(system, program_1_id);
        let _res = program_1.send(from, InitMessage::Distributor);

        let program_2 = Program::current_with_id(system, program_2_id);
        let _res = program_2.send(from, InitMessage::Distributor);

        let program_3 = Program::current_with_id(system, program_3_id);
        let _res = program_3.send(from, InitMessage::Distributor);

        let res = program_1.send(from, Request::Join(program_2_id.into()));
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        let res = program_1.send(from, Request::Join(program_3_id.into()));
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        (program_1, program_2, program_3)
    }

    #[test]
    fn composite_program() {
        let system = System::new();
        let (program_1, program_2, _program_3) = multi_program_setup(&system, 1, 2, 3);

        let from = 42;

        let res = program_1.send(from, Request::Receive(11));
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);

        assert!(res.contains(&log));

        let res = program_2.send(from, Request::Report);
        let log = Log::builder()
            .source(program_2.id())
            .dest(from)
            .payload(Reply::Amount(5));
        assert!(res.contains(&log));

        let res = program_1.send(from, Request::Report);
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Amount(11));
        assert!(res.contains(&log));
    }

    // This test show how RefCell will prevent to do conflicting changes (prevent multi-aliasing of the program state)
    #[test]
    fn conflicting_nodes() {
        let system = System::new();
        let (program_1, _program_2, _program_3) = multi_program_setup(&system, 1, 2, 3);

        let program_4_id = 4;
        let from = 42;

        let program_4 = Program::current_with_id(&system, program_4_id);
        let _res = program_4.send(from, InitMessage::Distributor);

        IntoIterator::into_iter([Request::Receive(11), Request::Join(program_4_id.into())])
            .map(|request| program_1.send(from, request))
            .zip(IntoIterator::into_iter([Reply::Success, Reply::Success]))
            .for_each(|(result, reply)| {
                let log = Log::builder()
                    .source(program_1.id())
                    .dest(from)
                    .payload(reply);
                // core::panic!("{:?}", result);
                assert!(result.contains(&log));
            });
    }
}
