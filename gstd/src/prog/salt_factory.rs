pub struct SaltFactory([u8; 32]);

fn invert(array: &mut [u8]) {
    for byte in array {
        *byte = 255 - *byte;
    }
}

impl SaltFactory {
    /// This is unsafe because all [`SaltFactory`] in one message from this constructor will produse same salts.
    pub unsafe fn new() -> Self {
        SaltFactory(*crate::msg::id().inner())
    }

    pub fn from_salt(mut salt: [u8; 32]) -> Self {
        invert(&mut salt);
        SaltFactory(sp_core_hashing::blake2_256(&salt))
    }

    pub fn generate(&mut self) -> [u8; 32] {
        let new_hash = sp_core_hashing::blake2_256(&self.0);
        let old_hash = self.0;
        self.0 = new_hash;
        old_hash
    }

    pub fn clone(&mut self) -> Self {
        let res = Self::from_salt(self.0);
        self.0 = sp_core_hashing::blake2_256(&self.0);
        res
    }
}
