// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::tests::utils::{
    EnvNetworkConfig, Node, NodeConfig, TestEnv, TestEnvConfig, TestingEventReceiver,
    ValidatorsConfig,
};

pub fn networked_config(validators: usize) -> TestEnvConfig {
    TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(validators),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    }
}

pub async fn start_validators(env: &mut TestEnv) -> Vec<Node> {
    let mut validators = Vec::new();
    for (index, validator) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{index}");
        let mut node =
            env.new_node(NodeConfig::named(format!("validator-{index}")).validator(validator));
        node.start_service().await;
        validators.push(node);
    }
    validators
}

pub async fn start_validators_with_events(
    env: &mut TestEnv,
) -> (Vec<Node>, Vec<TestingEventReceiver>) {
    let mut validators = Vec::new();
    let mut events = Vec::new();
    for (index, validator) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{index}");
        let mut node =
            env.new_node(NodeConfig::named(format!("validator-{index}")).validator(validator));
        node.start_service().await;
        events.push(node.new_events());
        validators.push(node);
    }
    (validators, events)
}
