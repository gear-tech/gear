use gear_core::ids::{prelude::CodeIdExt, CodeId, ProgramId};
use hypercore_db::Database;

pub trait DbContext {
    fn db(&self) -> Box<dyn Database>;
}

pub trait CodeContext {
    fn code(&self) -> &[u8];

    fn id(&self) -> CodeId {
        CodeId::generate(self.code())
    }

    fn len(&self) -> usize {
        self.code().len()
    }
}

pub struct VerifierContext {
    pub code: Vec<u8>,
}

impl CodeContext for VerifierContext {
    fn code(&self) -> &[u8] {
        &self.code
    }
}
