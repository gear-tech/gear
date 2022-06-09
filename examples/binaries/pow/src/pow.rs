use gstd::{Decode, Encode, TypeInfo};

#[derive(Debug, Encode, Decode, TypeInfo)]
pub struct Package {
    pub base: u8,
    pub exponent: u8,
    /// current exponent
    pub ptr: u8,
    /// the result of `pow(base, exponent)`
    pub result: u8,
}

impl Package {
    pub fn calc(mut self) -> Self {
        self.ptr += 1;
        self.result = self.base.saturating_mul(self.result);
        self
    }
}
