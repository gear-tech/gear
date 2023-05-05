// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

use gear_core::memory::{GearPage, PageBuf, PageU32Size};
use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use sp_core::{Decode, Encode};
use std::{env, fs, path::Path};

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
            page: page_number.raw(),
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
            GearPage::new(self.page)
                .unwrap_or_else(|_| panic!("Couldn't decode GearPage from u32: {}", self.page)),
            page_buf,
        )
    }
}

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

        fs::write(&path, data).unwrap_or_else(|_| panic!("Failed to write file {:?}", path));
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> ProgramMemoryDump {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(path)
            .clean();

        let json =
            fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path));

        serde_json::from_str(&json)
            .unwrap_or_else(|_| panic!("Failed to deserialize {:?} as ProgramMemoryDump", path))
    }
}
