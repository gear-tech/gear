use crate::{
    env::{BackendReport, Environment},
    error::ActorTerminationReason,
    mock::MockExt,
};
use gear_core::message::DispatchKind;
use gear_wasm_instrument::{
    gas_metering::CustomConstantCostRules,
    parity_wasm::{self, builder},
    InstrumentationBuilder, SyscallName,
};

/// Check that all syscalls are supported by backend.
#[test]
fn test_syscalls_table() {
    // Make module with one empty function.
    let mut module = builder::module()
        .function()
        .signature()
        .build()
        .build()
        .build();

    // Insert syscalls imports.
    for name in SyscallName::instrumentable() {
        let sign = name.signature();
        let types = module.type_section_mut().unwrap().types_mut();
        let type_no = types.len() as u32;
        types.push(parity_wasm::elements::Type::Function(sign.func_type()));

        module = builder::from_module(module)
            .import()
            .module("env")
            .external()
            .func(type_no)
            .field(name.to_str())
            .build()
            .build();
    }

    let module = InstrumentationBuilder::new("env")
        .with_gas_limiter(|_| CustomConstantCostRules::default())
        .instrument(module)
        .unwrap();
    let code = module.into_bytes().unwrap();

    // Execute wasm and check success.
    let ext = MockExt::default();
    let env =
        Environment::new(ext, &code, DispatchKind::Init, Default::default(), 0.into()).unwrap();
    let report = env.execute(|_, _| {}).unwrap();

    let BackendReport {
        termination_reason, ..
    } = report;

    assert_eq!(termination_reason, ActorTerminationReason::Success.into());
}
