
/// The result of charging gas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChargeResult {
    /// There was enough gas and it has been charged.
    Enough,
    /// There was not enough gas and it hasn't been charged.
    NotEnough,
}

/// Instrumentation error.
#[derive(Debug)]
pub enum InstrumentError {
    /// Error occured during decoding original program code.
    ///
    /// The provided code was a malformed Wasm bytecode or contained unsupported features
    /// (atomics, simd instructions, etc.).
    Decode,
    /// Error occured during injecting gas metering instructions.
    ///
    /// This might be due to program contained unsupported/non-deterministic instructionns
    /// (floats, manual memory grow, etc.).
    GasInjection,
    /// Error occured during encoding instrumented program.
    ///
    /// The only possible reason for that might be OOM.
    Encode,
}

/// Gas counter with unlimited gas.
#[derive(Debug)]
pub struct GasCounterUnlimited;

/// Gas counter with some predifined maximum gas.
#[derive(Debug)]
pub struct GasCounterLimited(pub u64);

/// Gas counter.
pub trait GasCounter {
    /// Charge some gas.
    fn charge(&mut self, val: u32) -> ChargeResult;
    /// Report how much gas is left.
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

/// Maximum theoretical gas limit.
#[cfg(test)]
pub fn max_gas() -> u64 {
    u64::max_value()
}
