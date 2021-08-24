use gear_core::message::MessageId;
use gear_core::program::ProgramId;

/// Blake2 Message Id Generator
pub struct BlakeMessageIdGenerator {
    pub program_id: ProgramId,
    pub nonce: u64,
}

impl gear_core::message::MessageIdGenerator for BlakeMessageIdGenerator {
    fn next(&mut self) -> MessageId {
        let mut data = self.program_id.as_slice().to_vec();
        data.extend(&self.nonce.to_le_bytes());

        self.nonce += 1;

        MessageId::from_slice(blake2_rfc::blake2b::blake2b(32, &[], &data).as_bytes())
    }

    fn current(&self) -> u64 {
        self.nonce
    }
}
