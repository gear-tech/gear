use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use gstd::{msg, MessageId, ProgramId};

// Persistent state (must be stored between blocks)
static mut MESSAGE_STATE: MessageState = MessageState::Idle;

#[derive(PartialEq)]
enum MessageState {
    Idle,
    Sent,
    WaitForReply,
}

pub struct MessageFuture;

impl Future for MessageFuture {
    type Output = Vec<u8>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match *state() {
            MessageState::Idle => Poll::Pending, // TODO: Unreachable, consider adding an assert here
            MessageState::Sent => {
                set_state(MessageState::WaitForReply);
                Poll::Pending
            }
            MessageState::WaitForReply => {
                if let Some(reply) = get_reply() {
                    Poll::Ready(reply)
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

/// Send a message and wait for reply.
pub fn send_and_wait_for_reply(
    program: ProgramId,
    payload: &[u8],
    gas_limit: u64,
    value: u128,
) -> MessageFuture {
    if *state() == MessageState::Idle {
        msg::send_with_value(program, payload, gas_limit, value);
        set_state(MessageState::Sent);
    }
    MessageFuture
}

fn get_reply() -> Option<Vec<u8>> {
    if msg::reply_to() != MessageId::default() {
        set_state(MessageState::Idle);
        return Some(msg::load());
    }
    None
}

#[inline]
fn state() -> &'static MessageState {
    unsafe { &MESSAGE_STATE }
}

#[inline]
fn set_state(state: MessageState) {
    unsafe {
        MESSAGE_STATE = state;
    }
}
