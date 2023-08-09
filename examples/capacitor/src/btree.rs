use crate::{Decode, Encode};
use alloc::collections::BTreeMap;
use gstd::{debug, msg, prelude::*};

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Request {
    Insert(u32, u32),
    Remove(u32),
    List,
    Clear,
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Reply {
    Error,
    None,
    Value(Option<u32>),
    List(Vec<(u32, u32)>),
}

pub(crate) type BTreeState = BTreeMap<u32, u32>;

pub(crate) fn init_btree() -> BTreeState {
    msg::reply((), 0).unwrap();
    BTreeMap::new()
}

pub(crate) fn handle_btree(state: &mut BTreeState) {
    let reply = msg::load_on_stack()
        .map(|request| process(state, request))
        .unwrap_or_else(|e| {
            debug!("Error processing request: {e:?}");
            Reply::Error
        });
    msg::reply(reply, 0).unwrap();
}

pub(crate) fn state_btree(state: BTreeState) {
    msg::reply(state, 0).unwrap();
}

fn process(state: &mut BTreeState, request: Request) -> Reply {
    use Request::*;

    match request {
        Insert(key, value) => Reply::Value(state.insert(key, value)),
        Remove(key) => Reply::Value(state.remove(&key)),
        List => Reply::List(state.iter().map(|(k, v)| (*k, *v)).collect()),
        Clear => {
            state.clear();
            Reply::None
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{Reply, Request};
    use alloc::vec;
    use gtest::{Log, Program, System};
    use crate::InitMessage;

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = 42;

        let res = program.send(from, InitMessage::BTree);
        let log = Log::builder().source(program.id()).dest(from);
        assert!(res.contains(&log));
    }

    #[test]
    fn simple() {
        let system = System::new();
        system.init_logger();

        let program = Program::current_opt(&system);

        let from = 42;

        let _res = program.send(from, InitMessage::BTree);

        IntoIterator::into_iter([
            Request::Insert(0, 1),
            Request::Insert(0, 2),
            Request::Insert(1, 3),
            Request::Insert(2, 5),
            Request::Remove(1),
            Request::List,
            Request::Clear,
            Request::List,
        ])
        .map(|r| program.send(from, r))
        .zip(IntoIterator::into_iter([
            Reply::Value(None),
            Reply::Value(Some(1)),
            Reply::Value(None),
            Reply::Value(None),
            Reply::Value(Some(3)),
            Reply::List(vec![(0, 2), (2, 5)]),
            Reply::None,
            Reply::List(vec![]),
        ]))
        .for_each(|(result, reply)| {
            let log = Log::builder()
                .source(program.id())
                .dest(from)
                .payload(reply);
            assert!(result.contains(&log));
        })
    }
}