use crate::{
    memory::{
        MemoryAccessManager, WasmMemoryRead, WasmMemoryReadAs, WasmMemoryReadDecoded,
        WasmMemoryWrite, WasmMemoryWriteAs,
    },
    mock::{MockExt, MockMemory},
};
use codec::{self, Decode, Encode, MaxEncodedLen};
use core::{fmt::Debug, marker::PhantomData};
use gear_core::{memory::Memory, pages::WASM_PAGE_SIZE};

const GAS_COUNTER: u64 = u64::MAX;

#[derive(Encode, Decode, MaxEncodedLen)]
#[codec(crate = codec)]
struct ZeroSizeStruct;

#[test]
fn test_pre_process_memory_accesses_with_no_accesses() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.pre_process_memory_accesses(&mut gas_counter);

    assert!(result.is_ok());
}

#[test]
fn test_pre_process_memory_accesses_with_only_reads() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_read(0, 10);

    let result = memory_access_manager.pre_process_memory_accesses(&mut gas_counter);

    assert!(result.is_ok());
    assert!(memory_access_manager.reads.is_empty());
}

#[test]
fn test_pre_process_memory_accesses_with_only_writes() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_write(0, 10);

    let result = memory_access_manager.pre_process_memory_accesses(&mut gas_counter);

    assert!(result.is_ok());
    assert!(memory_access_manager.writes.is_empty());
}

#[test]
fn test_pre_process_memory_accesses_with_reads_and_writes() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_read(0, 10);
    memory_access_manager.register_write(10, 20);

    let result = memory_access_manager.pre_process_memory_accesses(&mut gas_counter);

    assert!(result.is_ok());
    assert!(memory_access_manager.reads.is_empty());
    assert!(memory_access_manager.writes.is_empty());
}

#[test]
fn test_read_of_zero_size_buf() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    let memory = MockMemory::new(0);
    let read = memory_access_manager.register_read(0, 0);

    let result = memory_access_manager.read(&memory, read, &mut gas_counter);

    assert!(result.is_ok());
    assert_eq!(memory.read_attempt_count(), 0);
}

#[test]
fn test_read_of_zero_size_struct() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    let memory = MockMemory::new(0);
    let read = memory_access_manager.register_read_as::<ZeroSizeStruct>(0);

    let result = memory_access_manager.read_as(&memory, read, &mut gas_counter);

    assert!(result.is_ok());
    assert_eq!(memory.read_attempt_count(), 0);
}

#[test]
fn test_read_of_zero_size_encoded_value() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    let memory = MockMemory::new(0);
    let read = memory_access_manager.register_read_decoded::<ZeroSizeStruct>(0);

    let result = memory_access_manager.read_decoded(&memory, read, &mut gas_counter);

    assert!(result.is_ok());
    assert_eq!(memory.read_attempt_count(), 0);
}

#[test]
fn test_read_of_some_size_buf() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    let memory = MockMemory::new(1);
    let read = memory_access_manager.register_read(0, 10);

    let result = memory_access_manager.read(&memory, read, &mut gas_counter);

    assert!(result.is_ok());
    assert_eq!(memory.read_attempt_count(), 1);
}

#[test]
fn test_read_with_valid_memory_access() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_read(0, 10);

    let memory = &mut MockMemory::new(1);
    memory.write(0, &[5u8; 10]).unwrap();

    let result = memory_access_manager.read(
        memory,
        WasmMemoryRead { ptr: 0, size: 10 },
        &mut gas_counter,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), &[5u8; 10]);
}

#[test]
fn test_read_with_empty_memory_access() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.read(
        &MockMemory::new(10),
        WasmMemoryRead { ptr: 0, size: 0 },
        &mut gas_counter,
    );

    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_read_decoded_with_valid_encoded_data() {
    #[derive(Encode, Decode, Debug, PartialEq)]
    #[codec(crate = codec)]
    struct MockEncodeData {
        data: u64,
    }

    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_read_decoded::<u64>(0);

    let memory = &mut MockMemory::new(1);
    let encoded = MockEncodeData { data: 1234 }.encode();
    memory.write(0, &encoded).unwrap();

    let result = memory_access_manager.read_decoded::<MockMemory, u64>(
        memory,
        WasmMemoryReadDecoded {
            ptr: 0,
            _phantom: PhantomData,
        },
        &mut gas_counter,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1234u64);
}

#[test]
fn test_read_decoded_with_invalid_encoded_data() {
    #[derive(Debug)]
    struct InvalidDecode {}

    impl Decode for InvalidDecode {
        fn decode<T>(_input: &mut T) -> Result<Self, codec::Error> {
            Err("Invalid decoding".into())
        }
    }

    impl Encode for InvalidDecode {
        fn encode_to<T: codec::Output + ?Sized>(&self, _dest: &mut T) {}
    }

    impl MaxEncodedLen for InvalidDecode {
        fn max_encoded_len() -> usize {
            0
        }
    }

    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_read_decoded::<InvalidDecode>(0);

    let memory = &mut MockMemory::new(1);
    let encoded = alloc::vec![7u8; WASM_PAGE_SIZE];
    memory.write(0, &encoded).unwrap();

    let result = memory_access_manager.read_decoded::<MockMemory, InvalidDecode>(
        &MockMemory::new(1),
        WasmMemoryReadDecoded {
            ptr: 0,
            _phantom: PhantomData,
        },
        &mut gas_counter,
    );

    assert!(result.is_err());
}

#[test]
fn test_read_decoded_reading_error() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_read_decoded::<u64>(0);

    let result = memory_access_manager.read_decoded::<MockMemory, u64>(
        &MockMemory::new(1),
        WasmMemoryReadDecoded {
            ptr: u32::MAX,
            _phantom: PhantomData,
        },
        &mut gas_counter,
    );

    assert!(result.is_err());
}

#[test]
fn test_read_as_with_valid_data() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_read_as::<u64>(0);

    let memory = &mut MockMemory::new(1);
    let encoded = 1234u64.to_le_bytes();
    memory.write(0, &encoded).unwrap();

    let result = memory_access_manager.read_as::<MockMemory, u64>(
        memory,
        WasmMemoryReadAs {
            ptr: 0,
            _phantom: PhantomData,
        },
        &mut gas_counter,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1234);
}

#[test]
fn test_read_as_with_invalid_pointer() {
    let mut gas_counter = GAS_COUNTER;
    let memory = &mut MockMemory::new(1);

    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_read_as::<u64>(0);

    let result = memory_access_manager.read_as::<MockMemory, u128>(
        memory,
        WasmMemoryReadAs {
            ptr: u32::MAX,
            _phantom: PhantomData,
        },
        &mut gas_counter,
    );

    assert!(result.is_err());
}

#[test]
fn test_write_of_zero_size_buf() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    let mut memory = MockMemory::new(0);
    let write = memory_access_manager.register_write(0, 0);

    let result = memory_access_manager.write(&mut memory, write, &[], &mut gas_counter);

    assert!(result.is_ok());
    assert_eq!(memory.write_attempt_count(), 0);
}

#[test]
fn test_write_of_zero_size_struct() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    let mut memory = MockMemory::new(0);
    let write = memory_access_manager.register_write_as::<ZeroSizeStruct>(0);

    let result =
        memory_access_manager.write_as(&mut memory, write, ZeroSizeStruct, &mut gas_counter);

    assert!(result.is_ok());
    assert_eq!(memory.write_attempt_count(), 0);
}

#[test]
#[should_panic(expected = "buffer size is not equal to registered buffer size")]
fn test_write_with_zero_buffer_size() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_write(0, 10);

    let _ = memory_access_manager.write(
        &mut MockMemory::new(1),
        WasmMemoryWrite { ptr: 0, size: 10 },
        &[],
        &mut gas_counter,
    );
}

#[test]
fn test_write_of_some_size_buf() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    let mut memory = MockMemory::new(1);
    memory_access_manager.register_write(0, 10);
    let buffer = [0u8; 10];

    let result = memory_access_manager.write(
        &mut memory,
        WasmMemoryWrite { ptr: 0, size: 10 },
        &buffer,
        &mut gas_counter,
    );

    assert!(result.is_ok());
    assert_eq!(memory.write_attempt_count(), 1);
}

#[test]
fn test_write_with_larger_buffer_size() {
    extern crate std;

    let result = std::panic::catch_unwind(|| {
        let mut gas_counter = GAS_COUNTER;
        let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
        memory_access_manager.register_write(0, 10);
        let buffer = [0u8; 20];

        memory_access_manager.write(
            &mut MockMemory::new(1),
            WasmMemoryWrite { ptr: 0, size: 10 },
            &buffer,
            &mut gas_counter,
        )
    });

    assert!(result.is_err());
}

#[test]
fn test_write_as_with_zero_size_object() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_write_as::<u32>(0);

    let result = memory_access_manager.write_as(
        &mut MockMemory::new(1),
        WasmMemoryWriteAs::<u32> {
            ptr: 0,
            _phantom: PhantomData,
        },
        0,
        &mut gas_counter,
    );

    assert!(result.is_ok());
}

#[test]
fn test_write_as_with_same_object_size() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_write_as::<u8>(0);

    let result = memory_access_manager.write_as(
        &mut MockMemory::new(1),
        WasmMemoryWriteAs {
            ptr: 0,
            _phantom: PhantomData,
        },
        1u8,
        &mut gas_counter,
    );

    assert!(result.is_ok());
}

#[test]
fn test_write_as_with_larger_object_size() {
    let mut gas_counter = GAS_COUNTER;
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();
    memory_access_manager.register_write_as::<u8>(0);

    let result = memory_access_manager.write_as(
        &mut MockMemory::new(1),
        WasmMemoryWriteAs {
            ptr: WASM_PAGE_SIZE as u32,
            _phantom: PhantomData,
        },
        7u8,
        &mut gas_counter,
    );

    assert!(result.is_err());
}

#[test]
fn test_register_read_of_valid_interval() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_read(0, 10);

    assert_eq!(result.ptr, 0);
    assert_eq!(result.size, 10);
    assert_eq!(memory_access_manager.reads.len(), 1);
    assert_eq!(memory_access_manager.writes.len(), 0);
}

#[test]
fn test_register_read_of_zero_size_buf() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_read(0, 0);

    assert_eq!(result.ptr, 0);
    assert_eq!(result.size, 0);
    assert_eq!(memory_access_manager.reads.len(), 0);
}

#[test]
fn test_register_read_of_zero_size_struct() {
    let mut mem_access_manager = MemoryAccessManager::<()>::default();

    mem_access_manager.register_read_as::<ZeroSizeStruct>(142);

    assert_eq!(mem_access_manager.reads.len(), 0);
}

#[test]
fn test_register_read_of_zero_size_encoded_value() {
    let mut mem_access_manager = MemoryAccessManager::<()>::default();

    mem_access_manager.register_read_decoded::<ZeroSizeStruct>(142);

    assert_eq!(mem_access_manager.reads.len(), 0);
}

#[test]
fn test_register_read_as_with_valid_interval() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_read_as::<u8>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(memory_access_manager.reads.len(), 1);
    assert_eq!(memory_access_manager.writes.len(), 0);
    assert_eq!(memory_access_manager.reads[0].offset, 0);
    assert_eq!(
        memory_access_manager.reads[0].size,
        core::mem::size_of::<u8>() as u32
    );
}

#[test]
fn test_register_read_as_with_zero_size() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_read_as::<u8>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(memory_access_manager.reads.len(), 1);
    assert_eq!(memory_access_manager.writes.len(), 0);
    assert_eq!(memory_access_manager.reads[0].offset, 0);
    assert_eq!(
        memory_access_manager.reads[0].size,
        core::mem::size_of::<u8>() as u32
    );
}

#[derive(Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[codec(crate = codec)]
struct TestStruct {
    a: u32,
    b: u64,
}

#[test]
fn test_register_read_decoded_with_valid_interval() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_read_decoded::<TestStruct>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(memory_access_manager.reads.len(), 1);
    assert_eq!(memory_access_manager.writes.len(), 0);
    assert_eq!(memory_access_manager.reads[0].offset, 0);
    assert_eq!(
        memory_access_manager.reads[0].size,
        TestStruct::max_encoded_len() as u32
    );
}

#[test]
fn test_register_read_decoded_with_zero_size() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_read_decoded::<TestStruct>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(memory_access_manager.reads.len(), 1);
    assert_eq!(memory_access_manager.writes.len(), 0);
    assert_eq!(memory_access_manager.reads[0].offset, 0);
    assert_eq!(
        memory_access_manager.reads[0].size,
        TestStruct::max_encoded_len() as u32
    );
}

#[test]
fn test_register_write_of_valid_interval() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_write(0, 10);

    assert_eq!(result.ptr, 0);
    assert_eq!(result.size, 10);
    assert_eq!(memory_access_manager.reads.len(), 0);
    assert_eq!(memory_access_manager.writes.len(), 1);
}

#[test]
fn test_register_write_of_zero_size_buf() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_write(0, 0);

    assert_eq!(result.ptr, 0);
    assert_eq!(result.size, 0);
    assert_eq!(memory_access_manager.reads.len(), 0);
    assert_eq!(memory_access_manager.writes.len(), 0);
}

#[test]
fn test_register_write_of_zero_size_struct() {
    let mut mem_access_manager = MemoryAccessManager::<()>::default();

    mem_access_manager.register_write_as::<ZeroSizeStruct>(142);

    assert_eq!(mem_access_manager.writes.len(), 0);
}

#[test]
fn test_register_write_as_with_valid_interval() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_write_as::<u8>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(memory_access_manager.reads.len(), 0);
    assert_eq!(memory_access_manager.writes.len(), 1);
    assert_eq!(memory_access_manager.writes[0].offset, 0);
    assert_eq!(
        memory_access_manager.writes[0].size,
        core::mem::size_of::<u8>() as u32
    );
}

#[test]
fn test_register_write_as_with_zero_size() {
    let mut memory_access_manager = MemoryAccessManager::<MockExt>::default();

    let result = memory_access_manager.register_write_as::<u8>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(memory_access_manager.reads.len(), 0);
    assert_eq!(memory_access_manager.writes.len(), 1);
    assert_eq!(memory_access_manager.writes[0].offset, 0);
    assert_eq!(
        memory_access_manager.writes[0].size,
        core::mem::size_of::<u8>() as u32
    );
}

/// Check that all syscalls are supported by backend.
#[test]
fn test_syscalls_table() {
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
