// This file is part of Gear.
//
// Copyright (C) 2024-2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    abi::{IMirror, IMirrorWithInstrumentation},
    benchmarking::{ExecutionMode, context::SimulationContext},
};
use alloy::sol_types::SolConstructor;
use anyhow::{Result, anyhow, bail};
use revm::{
    ExecuteCommitEvm, ExecuteEvm, InspectEvm,
    context::TxEnv,
    context_interface::result::{ExecutionResult, Output},
    primitives::{Address, Bytes},
};

#[derive(Debug)]
pub enum MirrorImplKind {
    Regular,
    WithInstrumentation,
}

pub struct MirrorImpl {
    address: Address,
    mirror_impl_bytecode: Bytes,
    mirror_impl_with_instrumentation_bytecode: Bytes,
}

impl MirrorImpl {
    pub fn deploy(context: &mut SimulationContext, router_proxy: Address) -> Result<Self> {
        let (_, mirror_impl_bytecode) = Self::deploy_with_execution_mode(
            context,
            router_proxy,
            &IMirror::BYTECODE[..],
            ExecutionMode::Execute,
        )?;
        let (_, mirror_impl_with_instrumentation_bytecode) = Self::deploy_with_execution_mode(
            context,
            router_proxy,
            &IMirrorWithInstrumentation::BYTECODE[..],
            ExecutionMode::Execute,
        )?;

        let (mirror_impl, _) = Self::deploy_with_execution_mode(
            context,
            router_proxy,
            &IMirror::BYTECODE[..],
            ExecutionMode::ExecuteAndCommit,
        )?;

        Ok(Self {
            address: mirror_impl,
            mirror_impl_bytecode,
            mirror_impl_with_instrumentation_bytecode,
        })
    }

    fn deploy_with_execution_mode(
        context: &mut SimulationContext,
        router_proxy: Address,
        bytecode: &[u8],
        execution_mode: ExecutionMode,
    ) -> Result<(Address, Bytes)> {
        let tx = TxEnv::builder()
            .caller(context.deployer_address())
            .create()
            .data(
                [
                    bytecode,
                    &SolConstructor::abi_encode(&IMirror::constructorCall {
                        _router: router_proxy,
                    })[..],
                ]
                .concat()
                .into(),
            )
            .nonce(context.deployer_nonce())
            .build()
            .map_err(|_| anyhow!("failed to build TxEnv"))?;

        let execution_result = match execution_mode {
            ExecutionMode::Execute => context.evm().transact(tx)?.result,
            ExecutionMode::ExecuteAndCommit => context.evm().transact_commit(tx)?,
            ExecutionMode::ExecuteAndInspect => context.evm().inspect_tx(tx)?.result,
        };

        let ExecutionResult::Success {
            output: Output::Create(mirror_impl_bytecode, Some(mirror_impl)),
            ..
        } = execution_result
        else {
            bail!("failed to deploy Mirror contract");
        };

        if let ExecutionMode::ExecuteAndCommit = execution_mode {
            context.increment_deployer_nonce();
        }

        Ok((mirror_impl, mirror_impl_bytecode))
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub fn mirror_impl_bytecode(&self) -> &Bytes {
        &self.mirror_impl_bytecode
    }

    pub fn mirror_impl_with_instrumentation_bytecode(&self) -> &Bytes {
        &self.mirror_impl_with_instrumentation_bytecode
    }
}
