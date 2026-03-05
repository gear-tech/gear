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
    abi::{IRouter, IRouterWithInstrumentation},
    benchmarking::{ExecutionMode, SimulationContext},
};
use anyhow::{Result, anyhow, bail};
use revm::{
    ExecuteCommitEvm, ExecuteEvm, InspectEvm,
    context::TxEnv,
    context_interface::result::{ExecutionResult, Output},
    primitives::{Address, Bytes},
};

#[derive(Debug)]
pub enum RouterImplKind {
    Regular,
    WithInstrumentation,
}

pub struct RouterImpl {
    address: Address,
    router_impl_bytecode: Bytes,
    router_impl_with_instrumentation_bytecode: Bytes,
}

impl RouterImpl {
    pub fn deploy(context: &mut SimulationContext) -> Result<Self> {
        let (_, router_impl_bytecode) = Self::deploy_with_execution_mode(
            context,
            &IRouter::BYTECODE[..],
            ExecutionMode::Execute,
        )?;
        let (_, router_impl_with_instrumentation_bytecode) = Self::deploy_with_execution_mode(
            context,
            &IRouterWithInstrumentation::BYTECODE[..],
            ExecutionMode::Execute,
        )?;

        let (router_impl, _) = Self::deploy_with_execution_mode(
            context,
            &IRouter::BYTECODE[..],
            ExecutionMode::ExecuteAndCommit,
        )?;

        Ok(Self {
            address: router_impl,
            router_impl_bytecode,
            router_impl_with_instrumentation_bytecode,
        })
    }

    fn deploy_with_execution_mode(
        context: &mut SimulationContext,
        bytecode: &[u8],
        execution_mode: ExecutionMode,
    ) -> Result<(Address, Bytes)> {
        let tx = TxEnv::builder()
            .caller(context.deployer_address())
            .create()
            .data(Bytes::copy_from_slice(bytecode))
            .nonce(context.deployer_nonce())
            .build()
            .map_err(|_| anyhow!("failed to build TxEnv"))?;

        let execution_result = match execution_mode {
            ExecutionMode::Execute => context.evm().transact(tx)?.result,
            ExecutionMode::ExecuteAndCommit => context.evm().transact_commit(tx)?,
            ExecutionMode::ExecuteAndInspect => context.evm().inspect_tx(tx)?.result,
        };

        let ExecutionResult::Success {
            output: Output::Create(router_impl_bytecode, Some(router_impl)),
            ..
        } = execution_result
        else {
            bail!("failed to deploy Router contract");
        };

        if let ExecutionMode::ExecuteAndCommit = execution_mode {
            context.increment_deployer_nonce();
        }

        Ok((router_impl, router_impl_bytecode))
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub fn router_impl_bytecode(&self) -> &Bytes {
        &self.router_impl_bytecode
    }

    pub fn router_impl_with_instrumentation_bytecode(&self) -> &Bytes {
        &self.router_impl_with_instrumentation_bytecode
    }
}
