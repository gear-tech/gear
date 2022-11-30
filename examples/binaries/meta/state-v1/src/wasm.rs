use demo_meta_io::Wallet;
use gstd::{msg, prelude::*};

// Fn() -> Vec<Wallet>
#[no_mangle]
extern "C" fn all_wallets() {
    let wallets: Vec<Wallet> = msg::load().unwrap();
    msg::reply(wallets, 0).expect("Failed to share state");
}

// Fn() -> Option<Wallet>
#[no_mangle]
extern "C" fn first_wallet() {
    let wallets: Vec<Wallet> = msg::load().unwrap();
    msg::reply(wallets.first(), 0).expect("Failed to share state");
}

// Fn() -> Option<Wallet>
#[no_mangle]
extern "C" fn last_wallet() {
    let wallets: Vec<Wallet> = msg::load().unwrap();
    msg::reply(wallets.last(), 0).expect("Failed to share state");
}
