// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    BackendExternalities,
    error::{
        ActorTerminationReason, TerminationReason, TrapExplanation, UndefinedTerminationReason,
    },
};
use core::fmt::Debug;

pub type HostState<Ext, Mem> = Option<State<Ext, Mem>>;

pub struct State<Ext, Mem> {
    pub ext: Ext,
    pub memory: Mem,
    pub termination_reason: UndefinedTerminationReason,
}

impl<Ext: BackendExternalities, Mem> State<Ext, Mem> {
    /// Transforms [`Self`] into tuple of externalities, memory and
    /// termination reason returned after the execution.
    pub fn into_parts(self) -> (Ext, UndefinedTerminationReason) {
        let State {
            ext,
            termination_reason,
            ..
        } = self;
        (ext, termination_reason)
    }

    /// Terminates backend work after execution.
    ///
    /// The function handles `res`, which is the result of gear wasm
    /// program entry point invocation, and the termination reason.
    ///
    /// If the `res` is `Ok`, then execution considered successful
    /// and the termination reason will have the corresponding value.
    ///
    /// If the `res` is `Err`, then execution is considered to end
    /// with an error and the actual termination reason, which stores
    /// more precise information about the error, is returned.
    ///
    /// There's a case, when `res` is `Err`, but termination reason has
    /// a value for the successful ending of the execution. This is the
    /// case of calling `unreachable` panic in the program.
    pub fn terminate<T: Debug, WasmCallErr: Debug>(
        self,
        res: Result<T, WasmCallErr>,
        gas: u64,
    ) -> (Ext, TerminationReason) {
        log::trace!("Execution result = {res:?}");

        let (mut ext, termination_reason) = self.into_parts();
        let termination_reason = termination_reason.define(ext.current_counter_type());

        ext.decrease_current_counter_to(gas);

        let termination_reason = if res.is_err() {
            if matches!(
                termination_reason,
                TerminationReason::Actor(ActorTerminationReason::Success)
            ) {
                ActorTerminationReason::Trap(TrapExplanation::Unknown).into()
            } else {
                termination_reason
            }
        } else if matches!(
            termination_reason,
            TerminationReason::Actor(ActorTerminationReason::Success)
        ) {
            termination_reason
        } else {
            let err_msg = "State::terminate: Termination reason is not success, but executor successfully ends execution";

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        };

        (ext, termination_reason)
    }
}
