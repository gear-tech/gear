// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Integration tests for command `upload-code`.

use crate::common::{NodeExec, dev};
use anyhow::Result;

#[tokio::test]
async fn test_command_upload_code_works() -> Result<()> {
    let node = dev().await?;

    let output = node
        .gcli_with_stdin(["upload-code", "--stdin"], demo_fungible_token::WASM_BINARY)
        .await?;

    assert!(
        str::from_utf8(&output.stdout)?.contains("Successfully uploaded the code"),
        "code should be uploaded, but got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(())
}
