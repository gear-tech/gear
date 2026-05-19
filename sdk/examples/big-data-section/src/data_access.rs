// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::constants::*;

#[derive(Debug)]
pub struct DataAccess {
    array_index: u8,
    value_index: usize,
}

impl DataAccess {
    pub fn from_payload(payload: &[u8]) -> Result<Self, &'static str> {
        if payload.len() < 2 {
            return Err("Payload length must be at least 2 bytes");
        }

        Ok(Self {
            array_index: payload[0],
            value_index: payload[1..].iter().map(|&x| x as usize).sum::<usize>() % SIZE,
        })
    }

    pub fn constant(&self) -> i128 {
        match self.array_index {
            1 => ARRAY_1[self.value_index],
            2 => ARRAY_2[self.value_index],
            3 => ARRAY_3[self.value_index],
            4 => ARRAY_4[self.value_index],
            5 => ARRAY_5[self.value_index],
            6 => ARRAY_6[self.value_index],
            7 => ARRAY_7[self.value_index],
            _ => CONSTANT,
        }
    }
}
