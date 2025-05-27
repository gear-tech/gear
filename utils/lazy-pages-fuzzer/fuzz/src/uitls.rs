use std::mem;

pub fn string_to_hex(hex: &str) -> [u8; 32] {
    // Convert hex string to bytes
    let bytes = hex::decode(hex).expect("Invalid hex string");
    // Ensure the length is 32 bytes
    if bytes.len() != 32 {
        panic!("Hex string must be 32 bytes long");
    }
    // Convert bytes to array
    let mut array = [0u8; 32];
    array.copy_from_slice(&bytes);
    array
}

pub fn hex_to_string(bytes: &[u8; 32]) -> String {
    hex::encode(bytes)
}

// Convert a vector of u32 to a vector of u8
pub fn cast_vec(mut input: Vec<u32>) -> Vec<u8> {
    let ptr = input.as_mut_ptr();
    let length = input.len();
    let capacity = input.capacity();
    let _ = input.leak(); // Prevent Rust from freeing the memory
    unsafe {
        Vec::from_raw_parts(
            ptr as *mut u8,
            length * mem::size_of::<u32>(),
            capacity * mem::size_of::<u32>(),
        )
    }
}

pub fn cast_slice_mut(input: &mut [u32]) -> &mut [u8] {
    let ptr = input.as_mut_ptr();
    let length = input.len();
    unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, length * mem::size_of::<u32>()) }
}

pub fn cast_slice(input: &[u32]) -> &[u8] {
    let ptr = input.as_ptr();
    let length = input.len();
    unsafe { std::slice::from_raw_parts(ptr as *const u8, length * mem::size_of::<u32>()) }
}

pub fn simulate_panic(b: u8, bb: u8) {
    if b % 100 == 32 && bb % 100 == 42 {
        println!("{b}");
        eprint!("{bb}");
        panic!("Simulated panic in worker process");
    }
}
