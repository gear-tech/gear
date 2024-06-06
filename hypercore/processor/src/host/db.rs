use gear_core::{code::InstrumentedCode, ids::CodeId};
use hypercore_db::CASDatabase;
use parity_scale_codec::{Decode, Encode};
use primitive_types::H256;

pub(crate) struct Database {
    inner: Box<dyn CASDatabase>,
}

// TODO: consider to change decode panics to Results.
// TODO: consider to rename to StateStorage
impl Database {
    pub fn inner(&self) -> Box<dyn CASDatabase> {
        self.inner.clone_boxed()
    }

    pub fn new(inner: Box<dyn CASDatabase>) -> Self {
        Self { inner }
    }

    pub fn read<T: Decode>(&self, hash: &H256) -> Option<T> {
        self.inner
            .read(hash)
            .map(|data| T::decode(&mut &data[..]).expect("Failed to decode `T`"))
    }

    pub fn write<T: Encode>(&self, data: &T) -> H256 {
        let data = data.encode();
        self.inner.write(&data)
    }

    /// Read code section.
    pub fn read_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        self.inner.read(&code_id.into_bytes().into())
    }

    /// Write code section.
    pub fn write_code(&self, code_id: CodeId, code: &[u8]) {
        self.inner.write_by_hash(&code_id.into_bytes().into(), code);
    }

    /// Read instrumented code.
    pub fn read_instrumented_code(&self, hash: H256) -> Option<InstrumentedCode> {
        self.inner.read(&hash).map(|data| {
            InstrumentedCode::decode(&mut &data[..]).expect("Failed to decode InstrumentedCode")
        })
    }

    /// Write instrumented code.
    pub fn write_instrumented_code(&self, code: &InstrumentedCode) -> H256 {
        let data = code.encode();
        self.inner.write(&data)
    }
}
