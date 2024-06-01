use alloy::primitives::{hex, Address, FixedBytes};

const INITIALIZATION_CODE: FixedBytes<10> = FixedBytes::new(hex!("3d602d80600a3d3981f3"));
const RUNTIME_CODE_1: FixedBytes<10> = FixedBytes::new(hex!("363d3d373d3d3d363d73"));
const RUNTIME_CODE_2: FixedBytes<15> = FixedBytes::new(hex!("5af43d82803e903d91602b57fd5bf3"));

pub const fn minimal_proxy_bytecode(address: Address) -> FixedBytes<55> {
    let part1: FixedBytes<20> = INITIALIZATION_CODE.concat_const(RUNTIME_CODE_1);
    let part2: FixedBytes<40> = part1.concat_const(address.0);
    let part3: FixedBytes<55> = part2.concat_const(RUNTIME_CODE_2);
    part3
}
