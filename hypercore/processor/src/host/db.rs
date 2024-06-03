use crate::host::state::ProgramState;
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
    pub fn new(inner: Box<dyn CASDatabase>) -> Self {
        Self { inner }
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

    /// Read program state.
    pub fn read_state(&self, hash: H256) -> Option<ProgramState> {
        self.inner
            .read(&hash)
            .map(|data| ProgramState::decode(&mut &data[..]).expect("Failed to decode State"))
    }

    /// Write program state.
    pub fn write_state(&self, state: &ProgramState) -> H256 {
        let data = state.encode();
        self.inner.write(&data)
    }
}
