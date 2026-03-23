use gstd::{ActorId, msg};

static mut BENEFICIARY: ActorId = ActorId::zero();

#[unsafe(no_mangle)]
extern "C" fn init() {
    if msg::size() != 0 {
        // Set the beneficiary from the initialization payload
        let dest: ActorId = msg::load().expect("Failed to load init payload");
        unsafe { BENEFICIARY = dest };
    } else {
        // Original program creator becomes the beneficiary
        let dest = msg::source();
        unsafe { BENEFICIARY = dest };
    }

    gstd::debug!("Init, beneficiary: {:?}", unsafe { BENEFICIARY });
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let (_gas_limit, value): (u64, u128) = msg::load().expect("Failed to load handle payload");
    let value_receiver = unsafe { BENEFICIARY };

    gstd::debug!("Send value to beneficiary: {value_receiver:?}, value: {value}");

    #[cfg(feature = "ethexe")]
    msg::send_bytes(value_receiver, [], value).unwrap();
    #[cfg(not(feature = "ethexe"))]
    msg::send_bytes_with_gas(value_receiver, [], _gas_limit, value).unwrap();
}
