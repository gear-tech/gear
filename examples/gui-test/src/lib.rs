#![no_std]

use gstd::{msg, ext, prelude::*};
use scale_info::TypeInfo;
use codec::{Encode, Decode};
use core::convert::TryInto;

#[derive(Decode, TypeInfo)]
enum Action<A, B, C> {
    AVariant(A),
    BVariant(B),
    CVariant(C),
}

type Map = BTreeMap<String, u8>;

mod scope {
    pub type B = (Option<u8>, u128, [u8; 3]);
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


gstd::metadata!{
    title: "GUI test program",
    init:
        input: Action<AStruct, Option<CustomStruct<u8>>, Map>,
        output: Result<u8, Option<String>>,
    handle:
        input: (Map, Option<scope::B>),
        output: CustomStruct<Option<scope::B>>
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let incoming: Action<AStruct, Option<CustomStruct<u8>>, Map> = msg::load().expect("Unable to decode payload");

    let outgoing: Result<u8, Option<String>> = match incoming {
        Action::AVariant(a) => {
            let status = if a.online {
                "I am online"
            } else {
                "I am offline"
            };

            Err(Some(status.to_string()))
        },
        Action::BVariant(b) => {
            if let Some(inner) = b {
                Ok(inner.field)
            } else {
                Err(None)
            }
        },
        Action::CVariant(c) => {
            let count = c.keys().count();
            Ok(count.try_into().expect("Too much keys"))
        },
    };

    msg::reply(outgoing, 10_000_000, 555);
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    // let incoming: (Map, Option<scope::B>) = msg::load().expect("Unable to decode payload");

    // let outgoing = match incoming {
    //     (_m, Some(b)) => CustomStruct {
    //         field: Some(b),
    //     },
    //     (m, None) => {
    //         let count = m.keys().count();

    //         let opt_count = if let Ok(v) = count.try_into() {
    //             Some(v)
    //         } else {
    //             None
    //         };
            
    //         let b = (opt_count, 128u128, [0, 1, 2]);

    //         CustomStruct{
    //             field: Some(b),
    //         }
    //     }
    // };

    // msg::reply(outgoing, 10_000_000, 555);
}
