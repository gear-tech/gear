// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::State;
use gstd::{
    ActorId,
    errors::{ReplyCode, SimpleUnavailableActorError},
    exec, msg,
    prelude::*,
};

static mut STATE: Option<State> = None;

#[unsafe(no_mangle)]
extern "C" fn init() {
    let state: State = msg::load().expect("Failed to load state");
    unsafe { STATE = Some(state) };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let state = unsafe { STATE }.unwrap();
    match state {
        State::Exiting { inheritor } => exec::exit(inheritor),
        State::Assertive { send_to } => {
            msg::send(send_to, b"test", 0).unwrap();
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    let reply_code = msg::reply_code().unwrap();
    assert_eq!(
        reply_code,
        ReplyCode::error(SimpleUnavailableActorError::ProgramExited)
    );

    let inheritor = msg::load_bytes().unwrap();
    let inheritor = ActorId::try_from(inheritor.as_slice()).unwrap();
    assert_eq!(inheritor, exec::program_id());

    gstd::debug!("reply was asserted");
}
