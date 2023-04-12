#![no_std]
#![allow(deprecated)]

use codec::{Decode, Encode};
use core::convert::TryInto;
use gstd::{msg, prelude::*};
use scale_info::TypeInfo;

#[derive(Decode, TypeInfo)]
enum Action<A, B, C> {
    AVariant(A),
    BVar(B),
    CVariant(C),
}

#[derive(Decode, Encode, TypeInfo)]
struct AStruct {
    id: Vec<u8>,
    online: bool,
}

#[derive(Decode, Encode, TypeInfo)]
struct CustomStruct<T: Decode + Encode + TypeInfo> {
    field: T,
}

// NOTE: this macro has been deprecated, see
// https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta
gstd::metadata! {
    title: "GUI test program",
    init:
        input: Action<AStruct, Option<CustomStruct<u8>>, BTreeMap<String, u8>>,
        output: Result<u8, Option<String>>,
    handle:
        input: (BTreeMap<String, u8>, Option<(Option<u8>, u128, [u8; 3])>),
        output: CustomStruct<Option<(Option<u8>, u128, [u8; 3])>>,
}

type InitIncoming = Action<AStruct, Option<CustomStruct<u8>>, BTreeMap<String, u8>>;

#[no_mangle]
extern "C" fn init() {
    let incoming: InitIncoming = msg::load().expect("Unable to decode payload");

    let outgoing: Result<u8, Option<String>> = match incoming {
        Action::AVariant(a) => {
            let status = if a.online {
                "I am online"
            } else {
                "I am offline"
            };

            Err(Some(status.to_string()))
        }
        Action::BVar(b) => b.map(|inner| inner.field).ok_or(None),
        Action::CVariant(c) => {
            let count = c.keys().count();
            Ok(count.try_into().expect("Too much keys"))
        }
    };

    msg::reply(outgoing, 1_001_000).unwrap();
}

type HandleIncoming = (BTreeMap<String, u8>, Option<(Option<u8>, u128, [u8; 3])>);

#[no_mangle]
extern "C" fn handle() {
    let incoming: HandleIncoming = msg::load().expect("Unable to decode payload");

    let outgoing = match incoming {
        (_m, Some(b)) => CustomStruct { field: Some(b) },
        (m, None) => {
            let count = m.keys().count();

            let opt_count = count.try_into().ok();

            let arr: [u8; 3] = [0, 1, 2];

            let b = (opt_count, 128u128, arr);

            CustomStruct { field: Some(b) }
        }
    };

    msg::reply(outgoing, 1_001_000).unwrap();
}
