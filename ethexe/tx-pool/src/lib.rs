// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Ethexe transaction pool.

pub struct TranscationPool<Tx> {
    transactions: Vec<Tx>,
}

impl<Tx> TranscationPool<Tx> {
    pub fn new() -> Self {
        Self {
            transactions: Vec::new(),
        }
    }
}

impl<Tx: Transaction> TranscationPool<Tx> {
    pub fn add_new_transaction(&mut self, tx: Tx) -> Result<(), Tx::Error> {
        tx
            .validate()
            .map(|_| self.transactions.push(tx))
    }
}

pub trait Transaction {   
    type Error;
    fn validate(&self) -> Result<(), Self::Error>;
}

pub enum EthexeTransaction {
    Message {
        pub_key: Vec<u8>,
        raw_message: Vec<u8>,
        signed_message: Vec<u8>, 
    }
}

impl Transaction for EthexeTransaction {
    type Error = ();

    fn validate(&self) -> Result<(), Self::Error> {
        match self {
            EthexeTransaction::Message { pub_key, raw_message, signed_message } => todo!(),
        }
    }
}
