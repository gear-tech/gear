// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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
    abi::{IERC1967Proxy, IWrappedVara},
    benchmarking::SimulationContext,
};
use alloy::{
    dyn_abi::Eip712Domain,
    sol_types::{SolCall, SolConstructor, eip712_domain},
};
use anyhow::{Context, Result, anyhow, bail};
use revm::{
    ExecuteCommitEvm, ExecuteEvm,
    context::TxEnv,
    context_interface::result::{ExecutionResult, Output},
    primitives::{Address, Bytes, U256},
};

pub struct WrappedVara {
    impl_address: Address,
    proxy_address: Address,
}

impl WrappedVara {
    pub fn deploy(context: &mut SimulationContext) -> Result<Self> {
        let wrapped_vara_impl = Self::deploy_impl(context)?;
        let wrapped_vara_proxy = Self::deploy_proxy(context, wrapped_vara_impl)?;

        Ok(Self {
            impl_address: wrapped_vara_impl,
            proxy_address: wrapped_vara_proxy,
        })
    }

    fn deploy_impl(context: &mut SimulationContext) -> Result<Address> {
        let deployer_address = context.deployer_address();
        let deployer_nonce = context.deployer_nonce();

        let ExecutionResult::Success {
            output: Output::Create(_, Some(wrapped_vara_impl)),
            ..
        } = context.evm().transact_commit(
            TxEnv::builder()
                .caller(deployer_address)
                .create()
                .data(IWrappedVara::BYTECODE.clone())
                .nonce(deployer_nonce)
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to deploy WrappedVara contract");
        };

        context.increment_deployer_nonce();

        Ok(wrapped_vara_impl)
    }

    fn deploy_proxy(
        context: &mut SimulationContext,
        wrapped_vara_impl: Address,
    ) -> Result<Address> {
        let deployer_address = context.deployer_address();
        let deployer_nonce = context.deployer_nonce();

        let ExecutionResult::Success {
            output: Output::Create(_, Some(wrapped_vara_proxy)),
            ..
        } = context.evm().transact_commit(
            TxEnv::builder()
                .caller(deployer_address)
                .create()
                .data(
                    [
                        &IERC1967Proxy::BYTECODE[..],
                        &SolConstructor::abi_encode(&IERC1967Proxy::constructorCall {
                            implementation: wrapped_vara_impl,
                            _data: Bytes::copy_from_slice(
                                &IWrappedVara::initializeCall {
                                    initialOwner: deployer_address,
                                }
                                .abi_encode(),
                            ),
                        })[..],
                    ]
                    .concat()
                    .into(),
                )
                .nonce(deployer_nonce)
                .build()
                .map_err(|_| anyhow!("failed to build TxEnv"))?,
        )?
        else {
            bail!("failed to deploy TransparentUpgradeableProxy contract (WrappedVara proxy)");
        };

        context.increment_deployer_nonce();

        Ok(wrapped_vara_proxy)
    }

    pub fn impl_address(&self) -> Address {
        self.impl_address
    }

    pub fn proxy_address(&self) -> Address {
        self.proxy_address
    }

    pub(crate) fn nonces(&self, context: &mut SimulationContext, owner: Address) -> Result<U256> {
        let deployer_address = context.deployer_address();
        let deployer_nonce = context.deployer_nonce();

        let proxy_address = self.proxy_address();

        let ExecutionResult::Success {
            output: Output::Call(nonces),
            ..
        } = context
            .evm()
            .transact(
                TxEnv::builder()
                    .caller(deployer_address)
                    .call(proxy_address)
                    .data(IWrappedVara::noncesCall { owner }.abi_encode().into())
                    .nonce(deployer_nonce)
                    .build()
                    .map_err(|_| anyhow!("failed to build TxEnv"))?,
            )?
            .result
        else {
            bail!("failed to get nonces");
        };

        Ok(U256::from_be_slice(&nonces))
    }

    pub(crate) fn eip712_domain(&self, context: &mut SimulationContext) -> Result<Eip712Domain> {
        let deployer_address = context.deployer_address();
        let deployer_nonce = context.deployer_nonce();

        let proxy_address = self.proxy_address();

        let ExecutionResult::Success {
            output: Output::Call(eip712_domain),
            ..
        } = context
            .evm()
            .transact(
                TxEnv::builder()
                    .caller(deployer_address)
                    .call(proxy_address)
                    .data(IWrappedVara::eip712DomainCall {}.abi_encode().into())
                    .nonce(deployer_nonce)
                    .build()
                    .map_err(|_| anyhow!("failed to build TxEnv"))?,
            )?
            .result
        else {
            bail!("failed to get eip712 domain");
        };

        let Ok(eip712_domain) = IWrappedVara::eip712DomainCall::abi_decode_returns(&eip712_domain)
        else {
            bail!("failed to decode eip712 domain");
        };

        Ok(eip712_domain! {
            name: eip712_domain.name,
            version: eip712_domain.version,
            chain_id: eip712_domain.chainId.try_into().with_context(|| "chainId should fit into u64")?,
            verifying_contract: eip712_domain.verifyingContract,
        })
    }
}
