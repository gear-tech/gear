#![no_std]
#![feature(const_btree_new)]

extern crate alloc;

use alloc::collections::BTreeMap;
use codec::Decode;
use core::iter::{Cycle, Iterator};
use gstd::{debug, exec, msg, prelude::*};
use scale_info::TypeInfo;

const GAS_SPENT: u64 = 100_000_000;

static mut HANDLERS: Vec<Vec<Reply>> = vec![];
static mut HANDLER_MAP: BTreeMap<Payload, HandlerIterator> = BTreeMap::new();

enum HandlerIterator<'a> {
    Forward(ReplyIterator<'a>),
    Cycle(Cycle<ReplyIterator<'a>>),
}

impl<'a> Iterator for HandlerIterator<'a> {
    type Item = &'a Payload;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            HandlerIterator::Forward(i) => i.next(),
            HandlerIterator::Cycle(i) => i.next(),
        }
    }
}

#[derive(Clone)]
struct ReplyIterator<'a> {
    replies: &'a [Reply],
    reply_index: usize,
    index: usize,
}

impl<'a> Iterator for ReplyIterator<'a> {
    type Item = &'a Payload;

    fn next(&mut self) -> Option<Self::Item> {
        let reply_index = self.reply_index;
        let current_count = self.replies[reply_index].count as usize;
        if self.index >= current_count {
            return None;
        }

        let (new_reply_index, new_index) = {
            let new_index = self.index + 1;
            if new_index < current_count {
                (reply_index, new_index)
            } else {
                let new_reply_index = reply_index + 1;
                if new_reply_index < self.replies.len() {
                    (new_reply_index, 0)
                } else {
                    (reply_index, new_index)
                }
            }
        };

        let result = &self.replies[reply_index].reply;

        self.reply_index = new_reply_index;
        self.index = new_index;

        Some(result)
    }
}

type Payload = Vec<u8>;

#[derive(Debug, Decode, TypeInfo)]
struct Reply {
    reply: Payload,
    count: u32,
}

#[derive(Debug, Decode, TypeInfo)]
struct Handler {
    request: Payload,
    replies: Vec<Reply>,
    repeated: bool,
}

gstd::metadata! {
    title: "demo bot",
    init:
        input: Vec<Handler>,
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let reply = &HANDLER_MAP
        .get_mut(&msg::load_bytes())
        .map(|i| i.next())
        .flatten();
    if let Some(r) = reply {
        msg::reply_bytes(r, exec::gas_available() - GAS_SPENT, 0);
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let maybe_handlers: Result<Vec<Handler>, _> = msg::load();
    debug!("bot; maybe_handlers = {:?}", maybe_handlers);

    maybe_handlers
        .map_err(|_| {
            msg::reply(
                b"bot; failed to decode `Vec<Handler>`",
                exec::gas_available() - GAS_SPENT,
                0,
            )
        })
        .map(|v| {
            HANDLERS.reserve(v.len());
            v
        })
        .unwrap_or_else(|_| vec![])
        .into_iter()
        .filter(|h| !h.replies.is_empty())
        .for_each(|handler| {
            HANDLERS.push(handler.replies);
            HANDLER_MAP.insert(handler.request, {
                let iter = ReplyIterator {
                    replies: HANDLERS.last().unwrap(),
                    reply_index: 0,
                    index: 0,
                };

                if handler.repeated {
                    HandlerIterator::Cycle(iter.cycle())
                } else {
                    HandlerIterator::Forward(iter)
                }
            });
        });
}
