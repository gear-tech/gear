#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use ethereum_types::{Address, Bloom, H256, U256, BloomInput};
use rlp::{Encodable, RlpStream};
use codec::{Decode, Encode};

pub type Byte = u8;
pub type Bytes = Vec<Byte>;

#[derive(Clone, Debug, Decode, Encode)]
#[codec(crate = codec)]
pub struct Log {
    pub address: Address,
    pub topics: Vec<H256>,
    pub data: Bytes,
}

impl Log {
    pub fn calculate_bloom(&self) -> Bloom {
        self.topics.iter().fold(
            Bloom::from(BloomInput::Raw(self.address.as_bytes())),
            |mut bloom, topic| {
                bloom.accrue(BloomInput::Raw(topic.as_bytes()));
                bloom
            },
        )
    }
}

impl Encodable for Log {
    fn rlp_append(&self, rlp_stream: &mut RlpStream) {
        rlp_stream
            .begin_list(3)
            .append(&self.address)
            .append_list(&self.topics)
            .append(&self.data);
    }
}

#[derive(Clone, Debug, Decode, Encode)]
#[codec(crate = codec)]
pub struct Receipt {
    pub r#type: u64,
    pub status: bool,
    pub cumulative_gas_used: U256,
    pub logs: Vec<Log>,
    pub logs_bloom: Bloom,
}

// Transaction types.
pub const LEGACY_TX_TYPE: u64 = 0;
// 	AccessListTxType = 0x01
// 	DynamicFeeTxType = 0x02
// 	BlobTxType       = 0x03
// )

impl Encodable for Receipt {
    fn rlp_append(&self, rlp_stream: &mut RlpStream) {
        let rlp = rlp_stream.begin_list(4);
        match &self.status {
            true => rlp.append(&self.status),
            false => rlp.append_empty_data(),
        };
        rlp.append(&self.cumulative_gas_used)
            .append(&self.logs_bloom)
            .append_list(&self.logs);
    }
}

// according to https://github.com/ethereum/go-ethereum/issues/27062
pub fn rlp_encode_receipt(receipt: &Receipt) -> Bytes {
    let mut rlp_stream = RlpStream::new();
    rlp_stream.append(receipt);

    if receipt.r#type == LEGACY_TX_TYPE {
        return rlp_stream.out().to_vec();
    }
    
    let encoded = rlp_stream.out();

    let mut buf = Vec::with_capacity(encoded.len() + 1);
    buf.push(receipt.r#type as u8);
    buf.extend_from_slice(&encoded);

    buf
}
