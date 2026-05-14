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

use crate::{FTAction, FTEvent, InitConfig, IoFungibleToken};
use core::ops::Range;
use gstd::{ActorId, msg, prelude::*};
use hashbrown::HashMap;

const ZERO_ID: ActorId = ActorId::new([0u8; 32]);

#[derive(Debug, Clone, Default)]
struct FungibleToken {
    /// Name of the token.
    name: String,
    /// Symbol of the token.
    symbol: String,
    /// Total supply of the token.
    total_supply: u128,
    /// Map to hold balances of token holders.
    balances: HashMap<ActorId, u128>,
    /// Map to hold allowance information of token holders.
    allowances: HashMap<ActorId, HashMap<ActorId, u128>>,
    /// Token's decimals.
    pub decimals: u8,
}

static mut FUNGIBLE_TOKEN: Option<FungibleToken> = None;

impl FungibleToken {
    fn test_set(&mut self, user_ids: Range<u64>, amount: u128) {
        let len = user_ids.end - user_ids.start;
        self.total_supply += amount * len as u128;
        for user_id in user_ids {
            self.balances.insert(ActorId::from(user_id), amount);
        }
    }

    fn mint(&mut self, amount: u128) {
        let source = msg::source();
        self.balances
            .entry(source)
            .and_modify(|balance| *balance += amount)
            .or_insert(amount);
        self.total_supply += amount;
        msg::reply(
            FTEvent::Transfer {
                from: ZERO_ID,
                to: source,
                amount,
            },
            0,
        )
        .unwrap();
    }
    /// Executed on receiving `fungible-token-messages::BurnInput`.
    fn burn(&mut self, amount: u128) {
        let source = msg::source();
        if self.balances.get(&source).unwrap_or(&0) < &amount {
            panic!("Amount exceeds account balance");
        }
        self.balances
            .entry(source)
            .and_modify(|balance| *balance -= amount);
        self.total_supply -= amount;

        msg::reply(
            FTEvent::Transfer {
                from: source,
                to: ZERO_ID,
                amount,
            },
            0,
        )
        .unwrap();
    }
    /// Executed on receiving `fungible-token-messages::TransferInput` or `fungible-token-messages::TransferFromInput`.
    /// Transfers `amount` tokens from `sender` account to `recipient` account.
    fn transfer(&mut self, from: &ActorId, to: &ActorId, amount: u128) {
        if from == &ZERO_ID || to == &ZERO_ID {
            panic!("Zero addresses");
        };
        if !self.can_transfer(from, amount) {
            panic!("Not allowed to transfer")
        }
        if self.balances.get(from).unwrap_or(&0) < &amount {
            panic!("Amount exceeds account balance");
        }
        self.balances
            .entry(*from)
            .and_modify(|balance| *balance -= amount);
        self.balances
            .entry(*to)
            .and_modify(|balance| *balance += amount)
            .or_insert(amount);
        msg::reply(
            FTEvent::Transfer {
                from: *from,
                to: *to,
                amount,
            },
            0,
        )
        .unwrap();
    }

    /// Executed on receiving `fungible-token-messages::ApproveInput`.
    fn approve(&mut self, to: &ActorId, amount: u128) {
        if to == &ZERO_ID {
            panic!("Approve to zero address");
        }
        let source = msg::source();
        self.allowances
            .entry(source)
            .or_default()
            .insert(*to, amount);
        msg::reply(
            FTEvent::Approve {
                from: source,
                to: *to,
                amount,
            },
            0,
        )
        .unwrap();
    }

    fn can_transfer(&mut self, from: &ActorId, amount: u128) -> bool {
        let source = msg::source();
        if from == &source || self.balances.get(&source).unwrap_or(&0) >= &amount {
            return true;
        }
        if let Some(allowed_amount) = self.allowances.get(from).and_then(|m| m.get(&source))
            && allowed_amount >= &amount
        {
            self.allowances.entry(*from).and_modify(|m| {
                m.entry(source).and_modify(|a| *a -= amount);
            });
            return true;
        }

        false
    }
}

#[unsafe(no_mangle)]
extern "C" fn state() {
    let state = unsafe {
        static_mut!(FUNGIBLE_TOKEN)
            .take()
            .expect("State is not initialized")
    };
    let FungibleToken {
        name,
        symbol,
        total_supply,
        balances,
        allowances,
        decimals,
    } = state;

    let balances = balances.into_iter().collect();
    let allowances = allowances
        .into_iter()
        .map(|(id, allowance)| (id, allowance.into_iter().collect()))
        .collect();
    let payload = IoFungibleToken {
        name,
        symbol,
        total_supply,
        balances,
        allowances,
        decimals,
    };

    msg::reply(payload, 0)
        .expect("Failed to encode or reply with `<AppMetadata as Metadata>::State` from `state()`");
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let action: FTAction = msg::load().expect("Could not load Action");
    let ft: &mut FungibleToken =
        unsafe { static_mut!(FUNGIBLE_TOKEN).get_or_insert(Default::default()) };
    match action {
        FTAction::Mint(amount) => {
            ft.mint(amount);
        }
        FTAction::Burn(amount) => {
            ft.burn(amount);
        }
        FTAction::Transfer { from, to, amount } => {
            ft.transfer(&from, &to, amount);
        }
        FTAction::Approve { to, amount } => {
            ft.approve(&to, amount);
        }
        FTAction::TotalSupply => {
            msg::reply(FTEvent::TotalSupply(ft.total_supply), 0).unwrap();
        }
        FTAction::BalanceOf(account) => {
            let balance = ft.balances.get(&account).unwrap_or(&0);
            msg::reply(FTEvent::Balance(*balance), 0).unwrap();
        }
        FTAction::TestSet(user_ids, amount) => ft.test_set(user_ids, amount),
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let config: InitConfig = msg::load().expect("Unable to decode InitConfig");
    let ft = FungibleToken {
        name: config.name,
        symbol: config.symbol,
        decimals: config.decimals,
        balances: HashMap::with_capacity(config.initial_capacity.unwrap_or(0) as usize),
        ..Default::default()
    };
    unsafe { FUNGIBLE_TOKEN = Some(ft) };
}
