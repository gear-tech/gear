// Fungible Token Smart Contract.
// Implementation based on https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/token/ERC20/ERC20.sol

#![no_std]
#![allow(deprecated)]

use codec::{Decode, Encode};
use gstd::{debug, msg, prelude::*, ActorId};
use primitive_types::H256;
use scale_info::TypeInfo;

#[derive(Debug)]
struct FungibleToken {
    name: String,
    symbol: String,
    total_supply: u128,
    balances: BTreeMap<ActorId, u128>,
    allowances: BTreeMap<ActorId, BTreeMap<ActorId, u128>>,
}

static mut FUNGIBLE_TOKEN: FungibleToken = FungibleToken {
    name: String::new(),
    symbol: String::new(),
    total_supply: 0,
    balances: BTreeMap::new(),
    allowances: BTreeMap::new(),
};

impl FungibleToken {
    fn set_name(&mut self, name: String) {
        self.name = name;
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn set_symbol(&mut self, symbol: String) {
        self.symbol = symbol;
    }
    fn symbol(&self) -> &str {
        &self.symbol
    }
    #[allow(dead_code)]
    fn total_supply(&self) -> u128 {
        self.total_supply
    }
    #[allow(dead_code)]
    fn decimals(&self) -> u8 {
        18
    }
    fn increase_total_supply(&mut self, amount: u128) {
        self.total_supply = self.total_supply.saturating_add(amount);
    }
    fn decrease_total_supply(&mut self, amount: u128) {
        self.total_supply = self.total_supply.saturating_sub(amount);
    }
    fn set_balance(&mut self, account: &ActorId, amount: u128) {
        self.balances.insert(*account, amount);
    }
    fn get_balance(&self, account: &ActorId) -> u128 {
        *self.balances.get(account).unwrap_or(&0)
    }
    fn mint(&mut self, account: &ActorId, amount: u128) {
        let zero = ActorId::new(H256::zero().to_fixed_bytes());
        if account == &zero {
            panic!("FungibleToken: Mint to zero address.");
        }
        unsafe {
            self.increase_total_supply(amount);
            let old_balance = FUNGIBLE_TOKEN.get_balance(account);
            self.set_balance(account, old_balance.saturating_add(amount));
        }
        let transfer_data = TransferData {
            from: H256::zero(),
            to: H256::from_slice(account.as_ref()),
            amount,
        };
        msg::reply(Event::Transfer(transfer_data), 0).unwrap();
    }
    fn burn(&mut self, account: &ActorId, amount: u128) {
        let zero = ActorId::new(H256::zero().to_fixed_bytes());
        if account == &zero {
            panic!("FungibleToken: Burn from zero address.");
        }
        unsafe {
            self.decrease_total_supply(amount);
            let old_balance = FUNGIBLE_TOKEN.get_balance(account);
            self.set_balance(account, old_balance.saturating_sub(amount));
        }
        let transfer_data = TransferData {
            from: H256::from_slice(account.as_ref()),
            to: H256::zero(),
            amount,
        };
        msg::reply(Event::Transfer(transfer_data), 0).unwrap();
    }
    fn transfer(&mut self, sender: &ActorId, recipient: &ActorId, amount: u128) {
        let zero = ActorId::new(H256::zero().to_fixed_bytes());
        if sender == &zero {
            panic!("FungibleToken: Transfer from zero address.");
        }
        if recipient == &zero {
            panic!("FungibleToken: Transfer to zero address.");
        }
        let sender_balance = self.get_balance(sender);
        if amount > sender_balance {
            panic!("FungibleToken: Transfer amount exceeds balance.");
        }
        self.set_balance(sender, sender_balance.saturating_sub(amount));
        let recipient_balance = self.get_balance(recipient);
        self.set_balance(recipient, recipient_balance.saturating_add(amount));
        let transfer_data = TransferData {
            from: H256::from_slice(sender.as_ref()),
            to: H256::from_slice(recipient.as_ref()),
            amount,
        };
        msg::reply(Event::Transfer(transfer_data), 0).unwrap();
    }
    fn approve(&mut self, owner: &ActorId, spender: &ActorId, amount: u128) {
        let zero = ActorId::new(H256::zero().to_fixed_bytes());
        if owner == &zero {
            panic!("FungibleToken: Approve from zero address.");
        }
        if spender == &zero {
            panic!("FungibleToken: Approve to zero address.");
        }

        self.allowances
            .entry(*owner)
            .or_default()
            .insert(*spender, amount);
        let approve_data = ApproveData {
            owner: H256::from_slice(owner.as_ref()),
            spender: H256::from_slice(spender.as_ref()),
            amount,
        };
        msg::reply(Event::Approval(approve_data), 0).unwrap();
    }
    fn get_allowance(&self, owner: &ActorId, spender: &ActorId) -> u128 {
        *self
            .allowances
            .get(owner)
            .and_then(|m| m.get(spender))
            .unwrap_or(&0)
    }
    fn increase_allowance(&mut self, owner: &ActorId, spender: &ActorId, amount: u128) {
        let allowance = self.get_allowance(owner, spender);
        self.approve(owner, spender, allowance.saturating_add(amount));
    }
    fn decrease_allowance(&mut self, owner: &ActorId, spender: &ActorId, amount: u128) {
        let allowance = self.get_allowance(owner, spender);
        if amount > allowance {
            panic!("FungibleToken: Decreased allowance below zero.");
        }
        self.approve(owner, spender, allowance - amount);
    }
    fn transfer_from(
        &mut self,
        owner: &ActorId,
        sender: &ActorId,
        recipient: &ActorId,
        amount: u128,
    ) {
        let current_allowance = self.get_allowance(owner, sender);
        if current_allowance < amount {
            panic!("FungibleToken: Transfer amount exceeds allowance");
        }
        self.transfer(sender, recipient, amount);
        self.approve(owner, sender, current_allowance - amount);
    }
}

#[derive(Debug, Decode, TypeInfo)]
struct InitConfig {
    name: String,
    symbol: String,
}

#[derive(Debug, Decode, TypeInfo)]
struct MintInput {
    account: H256,
    amount: u128,
}

#[derive(Debug, Decode, TypeInfo)]
struct BurnInput {
    account: H256,
    amount: u128,
}

#[derive(Debug, Encode, Decode, TypeInfo)]
struct ApproveData {
    owner: H256,
    spender: H256,
    amount: u128,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
struct TransferData {
    from: H256,
    to: H256,
    amount: u128,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
struct TransferFromData {
    owner: H256,
    from: H256,
    to: H256,
    amount: u128,
}

#[derive(Debug, Decode, TypeInfo)]
enum Action {
    Mint(MintInput),
    Burn(BurnInput),
    Transfer(TransferData),
    TransferFrom(TransferFromData),
    Approve(ApproveData),
    IncreaseAllowance(ApproveData),
    DecreaseAllowance(ApproveData),
}

#[derive(Debug, Encode, TypeInfo)]
enum Event {
    Transfer(TransferData),
    Approval(ApproveData),
}

// NOTE: this macro has been deprecated, see
// https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta
gstd::metadata! {
    title: "FungibleToken",
        init:
            input : InitConfig,
        handle:
            input : Action,
            output : Event,
}

#[no_mangle]
extern "C" fn handle() {
    let action: Action = msg::load().expect("Could not load Action");
    let fungible_token = unsafe { &mut FUNGIBLE_TOKEN };

    match action {
        Action::Mint(mint_input) => {
            let to = ActorId::new(mint_input.account.to_fixed_bytes());
            fungible_token.mint(&to, mint_input.amount);
        }
        Action::Burn(burn_input) => {
            let from = ActorId::new(burn_input.account.to_fixed_bytes());
            fungible_token.burn(&from, burn_input.amount);
        }
        Action::Transfer(transfer_data) => {
            let from = ActorId::new(transfer_data.from.to_fixed_bytes());
            let to = ActorId::new(transfer_data.to.to_fixed_bytes());
            fungible_token.transfer(&from, &to, transfer_data.amount);
        }
        Action::Approve(approve_data) => {
            let owner = ActorId::new(approve_data.owner.to_fixed_bytes());
            let spender = ActorId::new(approve_data.spender.to_fixed_bytes());
            fungible_token.approve(&owner, &spender, approve_data.amount);
        }
        Action::TransferFrom(transfer_data) => {
            let owner = ActorId::new(transfer_data.owner.to_fixed_bytes());
            let from = ActorId::new(transfer_data.from.to_fixed_bytes());
            let to = ActorId::new(transfer_data.to.to_fixed_bytes());
            fungible_token.transfer_from(&owner, &from, &to, transfer_data.amount);
        }
        Action::IncreaseAllowance(approve_data) => {
            let owner = ActorId::new(approve_data.owner.to_fixed_bytes());
            let spender = ActorId::new(approve_data.spender.to_fixed_bytes());
            fungible_token.increase_allowance(&owner, &spender, approve_data.amount);
        }
        Action::DecreaseAllowance(approve_data) => {
            let owner = ActorId::new(approve_data.owner.to_fixed_bytes());
            let spender = ActorId::new(approve_data.spender.to_fixed_bytes());
            fungible_token.decrease_allowance(&owner, &spender, approve_data.amount)
        }
    }
}

#[no_mangle]
extern "C" fn init() {
    let config: InitConfig = msg::load().expect("Unable to decode InitConfig");
    debug!("FUNGIBLE_TOKEN {:?}", config);
    let fungible_token = unsafe { &mut FUNGIBLE_TOKEN };
    fungible_token.set_name(config.name);
    fungible_token.set_symbol(config.symbol);
    debug!(
        "FUNGIBLE_TOKEN {} SYMBOL {} created",
        fungible_token.name(),
        fungible_token.symbol()
    );
}
