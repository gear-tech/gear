// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct TestCase {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@time")]
    pub time: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TestSuite {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(default)]
    pub testcase: Vec<TestCase>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TestSuites {
    #[serde(rename = "@time")]
    pub time: String,
    #[serde(default)]
    pub testsuite: Vec<TestSuite>,
}
