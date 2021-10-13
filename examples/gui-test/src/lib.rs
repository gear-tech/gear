#![no_std]

use codec::{Decode, Encode};
use core::convert::TryInto;
use gstd::{msg, prelude::*};
use scale_info::TypeInfo;

#[derive(Decode, TypeInfo)]
enum Action<A, B, C> {
    AVariant(A),
    BVariant(B),
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

gstd::metadata! {
    title: "GUI test program",
    init:
        input: Action<AStruct, Option<CustomStruct<u8>>, BTreeMap<String, u8>>,
        output: Result<u8, Option<String>>,
    handle:
        input: (BTreeMap<String, u8>, Option<(Option<u8>, u128, [u8; 3])>),
        output: CustomStruct<Option<(Option<u8>, u128, [u8; 3])>>
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let incoming: Action<AStruct, Option<CustomStruct<u8>>, BTreeMap<String, u8>> =
        msg::load().expect("Unable to decode payload");

    let outgoing: Result<u8, Option<String>> = match incoming {
        Action::AVariant(a) => {
            let status = if a.online {
                "I am online"
            } else {
                "I am offline"
            };

            Err(Some(status.to_string()))
        }
        Action::BVariant(b) => {
            if let Some(inner) = b {
                Ok(inner.field)
            } else {
                Err(None)
            }
        }
        Action::CVariant(c) => {
            let count = c.keys().count();
            Ok(count.try_into().expect("Too much keys"))
        }
    };

    msg::send(0.into(), outgoing, 10_000_000, 555);
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let incoming: (BTreeMap<String, u8>, Option<(Option<u8>, u128, [u8; 3])>) =
        msg::load().expect("Unable to decode payload");

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

    msg::reply(outgoing, 10_000_000, 555);
}
