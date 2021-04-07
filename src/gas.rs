#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChargeResult {
    Enough,
    NotEnough,
}

#[derive(Debug)]
pub enum InstrumentError {
    Decode,
    GasInjection,
    Encode,
}

#[derive(Debug)]
pub struct GasCounterUnlimited;

#[derive(Debug)]
pub struct GasCounterLimited(pub u64);

pub trait GasCounter {
    fn charge(&mut self, val: u32) -> ChargeResult;
    fn left(&self) -> u64;
}

impl GasCounter for GasCounterUnlimited {
    fn charge(&mut self, _val: u32) -> ChargeResult {
        ChargeResult::Enough
    }

    fn left(&self) -> u64 { 0 }
}

impl GasCounter for GasCounterLimited {
    fn charge(&mut self, val: u32) -> ChargeResult {
        let val = val as u64;

        if self.0 < val {
            return ChargeResult::NotEnough;
        }

        self.0 -= val;

        ChargeResult::Enough
    }

    fn left(&self) -> u64 { self.0 }
}

/// Instrument code with gas-counting instructions.
pub fn instrument(code: &[u8]) -> Result<Vec<u8>, InstrumentError> {
    let module = parity_wasm::elements::Module::from_bytes(code)
        .map_err(|e| {
            log::error!("Error decoding module: {}", e);
            InstrumentError::Decode
        })?;

    let instrumented_module = pwasm_utils::inject_gas_counter(
        module,
        &pwasm_utils::rules::Set::new(
            // TODO: put into config/processing
            1000,
            Default::default()
        ).with_grow_cost(
            // TODO: prohibit grow competely somehow (manual allocation through host function is required)
            //       (by limits?)
            1000000
        ),
        "env",
    )
        .map_err(|_module| {
            log::error!("Error injecting gas counter");
            InstrumentError::GasInjection
        })?;


    parity_wasm::elements::serialize(instrumented_module)
        .map_err(|e| {
            log::error!("Error encoding module: {}", e);
            InstrumentError::Encode
        })
}

#[cfg(test)]
pub fn max_gas() -> u64 {
    u64::max_value()
}
