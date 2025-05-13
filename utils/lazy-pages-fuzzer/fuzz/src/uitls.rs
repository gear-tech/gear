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
    // Convert bytes to hex string
    hex::encode(bytes)
}
