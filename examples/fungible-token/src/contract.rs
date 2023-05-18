use ft_io::*;
use gmeta::Metadata;
use gstd::{debug, errors::Result as GstdResult, exec, msg, prelude::*, ActorId, MessageId};
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
    /// Executed on receiving `fungible-token-messages::MintInput`.
    fn mint(&mut self, amount: u128) {
        self.balances
            .entry(msg::source())
            .and_modify(|balance| *balance += amount)
            .or_insert(amount);
        self.total_supply += amount;
        msg::reply(
            FTEvent::Transfer {
                from: ZERO_ID,
                to: msg::source(),
                amount,
            },
            0,
        )
        .unwrap();
    }
    /// Executed on receiving `fungible-token-messages::BurnInput`.
    fn burn(&mut self, amount: u128) {
        if self.balances.get(&msg::source()).unwrap_or(&0) < &amount {
            panic!("Amount exceeds account balance");
        }
        self.balances
            .entry(msg::source())
            .and_modify(|balance| *balance -= amount);
        self.total_supply -= amount;

        msg::reply(
            FTEvent::Transfer {
                from: msg::source(),
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
        self.allowances
            .entry(msg::source())
            .or_default()
            .insert(*to, amount);
        msg::reply(
            FTEvent::Approve {
                from: msg::source(),
                to: *to,
                amount,
            },
            0,
        )
        .unwrap();
    }

    fn can_transfer(&mut self, from: &ActorId, amount: u128) -> bool {
        if from == &msg::source()
            || from == &exec::origin()
            || self.balances.get(&msg::source()).unwrap_or(&0) >= &amount
        {
            return true;
        }
        if let Some(allowed_amount) = self
            .allowances
            .get(from)
            .and_then(|m| m.get(&msg::source()))
        {
            if allowed_amount >= &amount {
                self.allowances.entry(*from).and_modify(|m| {
                    m.entry(msg::source()).and_modify(|a| *a -= amount);
                });
                return true;
            }
        }
        false
    }
}

fn common_state() -> <FungibleTokenMetadata as Metadata>::State {
    let state = static_mut_state();
    let FungibleToken {
        name,
        symbol,
        total_supply,
        balances,
        allowances,
        decimals,
    } = state.clone();

    let balances = balances.iter().map(|(k, v)| (*k, *v)).collect();
    let allowances = allowances
        .iter()
        .map(|(id, allowance)| (*id, allowance.iter().map(|(k, v)| (*k, *v)).collect()))
        .collect();
    IoFungibleToken {
        name,
        symbol,
        total_supply,
        balances,
        allowances,
        decimals,
    }
}

fn static_mut_state() -> &'static mut FungibleToken {
    unsafe { FUNGIBLE_TOKEN.get_or_insert(Default::default()) }
}

#[no_mangle]
extern "C" fn state() {
    reply(common_state())
        .expect("Failed to encode or reply with `<AppMetadata as Metadata>::State` from `state()`");
}

#[no_mangle]
extern "C" fn metahash() {
    let metahash: [u8; 32] = include!("../.metahash");
    reply(metahash).expect("Failed to encode or reply with `[u8; 32]` from `metahash()`");
}

fn reply(payload: impl Encode) -> GstdResult<MessageId> {
    msg::reply(payload, 0)
}

#[no_mangle]
extern "C" fn handle() {
    let action: FTAction = msg::load().expect("Could not load Action");
    let ft: &mut FungibleToken = unsafe { FUNGIBLE_TOKEN.get_or_insert(Default::default()) };
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
    }
}

#[no_mangle]
extern "C" fn init() {
    let config: InitConfig = msg::load().expect("Unable to decode InitConfig");
    let ft = FungibleToken {
        name: config.name,
        symbol: config.symbol,
        decimals: config.decimals,
        ..Default::default()
    };
    unsafe { FUNGIBLE_TOKEN = Some(ft) };
}

#[no_mangle]
extern "C" fn meta_state() -> *mut [i32; 2] {
    let query: State = msg::load().expect("failed to decode input argument");
    let ft: &mut FungibleToken = unsafe { FUNGIBLE_TOKEN.get_or_insert(Default::default()) };
    debug!("{:?}", query);
    let encoded = match query {
        State::Name => StateReply::Name(ft.name.clone()),
        State::Symbol => StateReply::Name(ft.symbol.clone()),
        State::Decimals => StateReply::Decimals(ft.decimals),
        State::TotalSupply => StateReply::TotalSupply(ft.total_supply),
        State::BalanceOf(account) => {
            let balance = ft.balances.get(&account).unwrap_or(&0);
            StateReply::Balance(*balance)
        }
    }
    .encode();
    gstd::util::to_leak_ptr(encoded)
}

#[derive(Debug, Encode, Decode, TypeInfo)]
#[codec(crate = gstd::codec)]
#[scale_info(crate = gstd::scale_info)]
pub enum State {
    Name,
    Symbol,
    Decimals,
    TotalSupply,
    BalanceOf(ActorId),
}

#[derive(Debug, Encode, Decode, TypeInfo)]
#[codec(crate = gstd::codec)]
#[scale_info(crate = gstd::scale_info)]
pub enum StateReply {
    Name(String),
    Symbol(String),
    Decimals(u8),
    TotalSupply(u128),
    Balance(u128),
}
