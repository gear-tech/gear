#![no_std]
#![allow(deprecated)]

//!
//! Hi! I'm bot. I can be configured in `init` and then reply
//! with fixed payload to the corresponding requests. Requests with
//! no handler will not be responded.
//!
//! [`init`] method gets a handler list - [`Handler`]. `Handler` contains
//! `request`, predefined `reply` list and the flag `repeated` - if the
//! processing reply list should be repeated. Handlers are independent.
//!
//! `replies` is a list of [`Reply`]. `Reply` consists of the reply-payload
//! and the count which defines how many times the reply should be sent.
//! The idea is similiar to how dash pattern is set for line drawing in
//! various GUI-frameworks.
//!
//! For example, I am able to play the usual ping/pong:
//! ```yaml
//! init_message:
//! kind: custom
//! value:
//!   # "PING"
//!   - request: "0x50494e47"
//!     repeated: true
//!     replies:
//!       - count: 1
//!       # "PONG"
//!         reply: "0x504f4e47"
//! ```
//!
//! Ping/pong with two different responses:
//! ```yaml
//! init_message:
//! kind: custom
//! value:
//!   # "PING"
//!   - request: "0x50494e47"
//!     repeated: true
//!     replies:
//!       - count: 1
//!       # "First PONG"
//!         reply: "0x466972737420504f4e47"
//!       - count: 1
//!       # "Second PONG"
//!         reply: "0x5365636f6e6420504f4e47"
//! ```
//!
//! Ping/pong with defined first answer:
//! ```yaml
//! init_message:
//! kind: custom
//! value:
//!   # "PING"
//!   - request: "0x50494e47"
//!     repeated: false
//!     replies:
//!       - count: 1
//!       # "The very first PONG"
//!         reply: "0x546865207665727920666972737420504f4e47"
//!       # u32::max
//!       - count: 4294967295
//!       # "PONG"
//!         reply: "0x504f4e47"
//! ```
//!
//! Note that this handler is not repeated but the count for second reply is
//! big enough to emulate an endless loop.
//!
//! Feel free to experiment with various Handlers. Hope I can be helpful.
//!

extern crate alloc;

use alloc::collections::BTreeMap;
use codec::Decode;
use core::iter::{Cycle, Iterator};
use gstd::{msg, prelude::*};
use scale_info::TypeInfo;

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
pub struct Reply {
    reply: Payload,
    count: u32,
}

#[derive(Debug, Decode, TypeInfo)]
pub struct Handler {
    request: Payload,
    replies: Vec<Reply>,
    repeated: bool,
}

// NOTE: this macro has been deprecated, see
// https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta
gstd::metadata! {
    title: "demo bot",
    init:
        input: Vec<Handler>,
}

#[no_mangle]
extern "C" fn handle() {
    let reply = unsafe {
        &HANDLER_MAP
            .get_mut(&msg::load_bytes().expect("Failed to load payload bytes"))
            .and_then(|i| i.next())
    };
    if let Some(r) = reply {
        msg::reply_bytes(r, 0).unwrap();
    }
}

#[no_mangle]
extern "C" fn init() {
    let maybe_handlers: Result<Vec<Handler>, _> = msg::load();

    maybe_handlers
        .map_err(|_| msg::reply(b"bot; failed to decode `Vec<Handler>`", 0).unwrap())
        .map(|v| {
            unsafe { HANDLERS.reserve(v.len()) };
            v
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|h| !h.replies.is_empty())
        .for_each(|handler| unsafe {
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
