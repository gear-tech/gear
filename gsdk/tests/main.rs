// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use gsdk::Api;

#[tokio::test]
async fn timeout() {
    let error = Api::new_with_timeout(None, 0).await.err();
    // NOTE:
    //
    // There are two kinds of timeout error provided by subxt:
    //
    // - client request timeout
    // - transport timeout
    assert!(
        format!("{error:?}").to_lowercase().contains("timeout"),
        "Unexpected error occurred: {error:?}"
    );
}
