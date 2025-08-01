// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Useful utilities needed for testing and other stuff.

use gear_core::{memory::PageBuf, pages::GearPage};
pub use nonempty::NonEmpty;
use parity_scale_codec::{Decode, Encode};
use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub mod codegen;

/// Trait describes a collection which can get a value by it's index.
/// The index can be in any range, even [length(implementor), ..).
///
/// The key feature of the trait is that the implementor should guarantee
/// that with under provided index there's always some value. The best way
/// to do that is to implement a trait for a guaranteed not empty type.
pub trait RingGet<V> {
    /// Returns with a guarantee a value under `index`.
    fn ring_get(&self, index: usize) -> &V;
}

impl<V> RingGet<V> for NonEmpty<V> {
    fn ring_get(&self, index: usize) -> &V {
        // Guaranteed to have value, because index is in the range of [0; self.len()).
        self.get(index % self.len()).expect("guaranteed to be some")
    }
}

/// Returns time elapsed since [`UNIX_EPOCH`] in milliseconds.
pub fn now_millis() -> u64 {
    now_duration().as_millis() as u64
}

/// Returns time elapsed since [`UNIX_EPOCH`] in microseconds.
pub fn now_micros() -> u128 {
    now_duration().as_micros()
}

/// Returns [`Duration`] from [`UNIX_EPOCH`] until now.
pub fn now_duration() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Internal error: current time before UNIX Epoch")
}

/// Initialize a simple logger from env.
///
/// Does show:
/// - level
/// - timestamp
/// - module
///
/// Does not show
/// - module path
pub fn init_default_logger() {
    let _ = tracing_subscriber::fmt::try_init();
}

/// Stores one memory page dump as the hex string.
#[derive(Serialize, Deserialize)]
pub struct MemoryPageDump {
    page: u32,
    data: Option<String>,
}

impl MemoryPageDump {
    pub fn new(page_number: GearPage, page_data: PageBuf) -> MemoryPageDump {
        let mut data_vec = vec![];
        page_data.encode_to(&mut data_vec);
        let data = data_vec
            .iter()
            .any(|&byte| byte != 0)
            .then(|| hex::encode(data_vec));

        MemoryPageDump {
            page: page_number.into(),
            data,
        }
    }

    pub fn into_gear_page(self) -> (GearPage, PageBuf) {
        let page_buf = if let Some(page_hex) = self.data {
            let data = hex::decode(page_hex).expect("Unexpected memory page data encoding");
            PageBuf::decode(&mut &*data).expect("Invalid PageBuf data found")
        } else {
            PageBuf::new_zeroed()
        };
        (
            GearPage::try_from(self.page)
                .unwrap_or_else(|_| panic!("Couldn't decode GearPage from u32: {}", self.page)),
            page_buf,
        )
    }
}

/// Stores all program's page dumps and it's balance.
#[derive(Serialize, Deserialize, Default)]
pub struct ProgramMemoryDump {
    pub balance: u128,
    pub reserved_balance: u128,
    pub pages: Vec<MemoryPageDump>,
}

impl ProgramMemoryDump {
    pub fn save_to_file(&self, path: impl AsRef<Path>) {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(path)
            .clean();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|_| panic!("Couldn't create folder {}", parent.display()));
        }

        let data =
            serde_json::to_string(&self).expect("Failed to serialize ProgramMemoryDump to JSON");

        fs::write(&path, data).unwrap_or_else(|_| panic!("Failed to write file {path:?}"));
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> ProgramMemoryDump {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(path)
            .clean();

        let json =
            fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read file {path:?}"));

        serde_json::from_str(&json)
            .unwrap_or_else(|_| panic!("Failed to deserialize {path:?} as ProgramMemoryDump"))
    }
}
