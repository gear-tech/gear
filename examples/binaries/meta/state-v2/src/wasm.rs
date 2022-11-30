use demo_meta_io::{Id, Person, Wallet};
use gstd::{msg, prelude::*};

// Fn(Id) -> Option<Wallet>
#[no_mangle]
extern "C" fn wallet_by_id() {
    let (id, wallets): (Id, Vec<Wallet>) = msg::load().unwrap();

    let res = wallets.into_iter().find(|w| w.id == id);

    msg::reply(res, 0).expect("Failed to share state");
}

// Fn(Person) -> Option<Wallet>
#[no_mangle]
extern "C" fn wallet_by_person() {
    let (person, wallets): (Person, Vec<Wallet>) = msg::load().unwrap();

    let res = wallets.into_iter().find(|w| w.person == person);

    msg::reply(res, 0).expect("Failed to share state");
}
