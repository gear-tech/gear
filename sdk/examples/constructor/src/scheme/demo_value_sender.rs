use crate::Calls;

#[derive(Debug)]
pub struct TestData {
    // For request data.
    pub gas_limit: Option<u64>,
    pub value: u128,
    // Extra data (especially for gasless sending).
    pub gas_limit_to_send: u64,
    pub extra_gas: u64,
}

impl TestData {
    pub fn gasless(value: u128, mailbox_threshold: u64) -> Self {
        Self {
            gas_limit: None,
            value,
            gas_limit_to_send: mailbox_threshold,
            extra_gas: mailbox_threshold * 5,
        }
    }

    pub fn gasful(gas_limit: u64, value: u128) -> Self {
        Self {
            gas_limit: Some(gas_limit),
            value,
            gas_limit_to_send: gas_limit,
            extra_gas: 0,
        }
    }

    pub fn request(&self, account_id: impl Into<[u8; 32]>) -> Calls {
        if let Some(gas_limit) = self.gas_limit {
            Calls::builder().send_value_wgas(account_id.into(), [], gas_limit, self.value)
        } else {
            Calls::builder().send_value(account_id.into(), [], self.value)
        }
    }
}
