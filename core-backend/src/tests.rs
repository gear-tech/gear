use crate::{
    error::UndefinedTerminationReason,
    memory::{MemoryWrapRef, WasmMemoryReadAs, WasmMemoryReadDecoded, WasmMemoryWriteAs},
    mock::{MockExt, MockMemory, PreProcessMemoryAccesses},
    runtime::CallerWrap,
    state::{HostState, State},
};
use codec::{self, Decode, Encode, MaxEncodedLen};
use core::{fmt::Debug, marker::PhantomData};
use gear_core::{memory::Memory, pages::WASM_PAGE_SIZE};
use gear_sandbox::{default_executor::Store, SandboxStore};

type MemoryAccessRegistrar =
    crate::memory::MemoryAccessRegistrar<Store<HostState<MockExt, MockMemory>>>;
type MemoryAccessIo<'a> = crate::memory::MemoryAccessIo<
    MemoryWrapRef<'a, Store<HostState<MockExt, MockMemory>>, MockMemory>,
>;

#[derive(Encode, Decode, MaxEncodedLen)]
#[codec(crate = codec)]
struct ZeroSizeStruct;

fn new_store() -> Store<HostState<MockExt, MockMemory>> {
    Store::new(Some(State {
        ext: MockExt::default(),
        memory: MockMemory::new(0),
        termination_reason: UndefinedTerminationReason::ProcessAccessErrorResourcesExceed,
    }))
}

#[test]
fn test_pre_process_with_no_accesses() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let registrar = MemoryAccessRegistrar::default();
    let _io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
}

#[test]
fn test_pre_process_with_only_reads() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let mut registrar = MemoryAccessRegistrar::default();
    registrar.register_read(0, 10);

    let _io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();

    PreProcessMemoryAccesses::with(|accesses| {
        assert_eq!(accesses.reads.len(), 1);
    });
}

#[test]
fn test_pre_process_with_only_writes() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let mut registrar = MemoryAccessRegistrar::default();
    registrar.register_write(0, 10);

    let _io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    PreProcessMemoryAccesses::with(|accesses| {
        assert_eq!(accesses.writes.len(), 1);
    });
}

#[test]
fn test_pre_process_with_reads_and_writes() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let mut registrar = MemoryAccessRegistrar::default();
    registrar.register_read(0, 10);
    registrar.register_write(10, 20);

    let _io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    PreProcessMemoryAccesses::with(|accesses| {
        assert_eq!(accesses.reads.len(), 1);
        assert_eq!(accesses.writes.len(), 1);
    });
}

#[test]
fn test_read_of_zero_size_buf() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let mut registrar = MemoryAccessRegistrar::default();
    let read = registrar.register_read(0, 0);
    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.read(read).unwrap();

    assert_eq!(caller_wrap.host_state_mut().memory.read_attempt_count(), 0);
}

#[test]
fn test_read_of_zero_size_struct() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let mut registrar = MemoryAccessRegistrar::default();
    let read = registrar.register_read_as::<ZeroSizeStruct>(0);

    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.read_as(read).unwrap();

    assert_eq!(caller_wrap.host_state_mut().memory.read_attempt_count(), 0);
}

#[test]
fn test_read_of_zero_size_encoded_value() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let mut registrar = MemoryAccessRegistrar::default();
    let read = registrar.register_read_decoded::<ZeroSizeStruct>(0);
    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.read_decoded(read).unwrap();
    assert_eq!(caller_wrap.host_state_mut().memory.read_attempt_count(), 0);
}

#[test]
fn test_read_of_some_size_buf() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    caller_wrap.host_state_mut().memory = MockMemory::new(1);

    let mut registrar = MemoryAccessRegistrar::default();
    let read = registrar.register_read(0, 10);
    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.read(read).unwrap();

    assert_eq!(caller_wrap.host_state_mut().memory.read_attempt_count(), 1);
}

#[test]
fn test_read_with_valid_memory_access() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    let memory = &mut caller_wrap.host_state_mut().memory;
    *memory = MockMemory::new(1);
    memory.write(0, &[5u8; 10]).unwrap();

    let mut registrar = MemoryAccessRegistrar::default();
    let read = registrar.register_read(0, 10);

    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    let vec = io.read(read).unwrap();
    assert_eq!(vec.as_slice(), &[5u8; 10]);
}

#[test]
fn test_read_decoded_with_valid_encoded_data() {
    #[derive(Encode, Decode, Debug, PartialEq)]
    #[codec(crate = codec)]
    struct MockEncodeData {
        data: u64,
    }

    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    let memory = &mut caller_wrap.host_state_mut().memory;
    *memory = MockMemory::new(1);
    let encoded = MockEncodeData { data: 1234 }.encode();
    memory.write(0, &encoded).unwrap();

    let mut registrar = MemoryAccessRegistrar::default();
    let read = registrar.register_read_decoded::<u64>(0);
    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    let data: u64 = io.read_decoded(read).unwrap();
    assert_eq!(data, 1234u64);
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

    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    let memory = &mut caller_wrap.host_state_mut().memory;
    *memory = MockMemory::new(1);
    let encoded = alloc::vec![7u8; WASM_PAGE_SIZE];
    memory.write(0, &encoded).unwrap();

    let mut registrar = MemoryAccessRegistrar::default();
    let read = registrar.register_read_decoded::<InvalidDecode>(0);
    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.read_decoded::<InvalidDecode>(read).unwrap_err();
}

#[test]
fn test_read_decoded_reading_error() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    caller_wrap.host_state_mut().memory = MockMemory::new(1);
    let mut registrar = MemoryAccessRegistrar::default();
    let _read = registrar.register_read_decoded::<u64>(0);
    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.read_decoded::<u64>(WasmMemoryReadDecoded {
        ptr: u32::MAX,
        _phantom: PhantomData,
    })
    .unwrap_err();
}

#[test]
fn test_read_as_with_valid_data() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let memory = &mut caller_wrap.host_state_mut().memory;
    *memory = MockMemory::new(1);
    let encoded = 1234u64.to_le_bytes();
    memory.write(0, &encoded).unwrap();

    let mut registrar = MemoryAccessRegistrar::default();
    let read = registrar.register_read_as::<u64>(0);
    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    let decoded = io.read_as::<u64>(read).unwrap();
    assert_eq!(decoded, 1234);
}

#[test]
fn test_read_as_with_invalid_pointer() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    caller_wrap.host_state_mut().memory = MockMemory::new(1);

    let mut registrar = MemoryAccessRegistrar::default();
    let _read = registrar.register_read_as::<u64>(0);
    let io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.read_as::<u128>(WasmMemoryReadAs {
        ptr: u32::MAX,
        _phantom: PhantomData,
    })
    .unwrap_err();
}

#[test]
fn test_write_of_zero_size_buf() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let mut registrar = MemoryAccessRegistrar::default();
    let write = registrar.register_write(0, 0);
    let mut io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.write(write, &[]).unwrap();

    assert_eq!(caller_wrap.host_state_mut().memory.write_attempt_count(), 0);
}

#[test]
fn test_write_of_zero_size_struct() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let mut registrar = MemoryAccessRegistrar::default();
    let write = registrar.register_write_as::<ZeroSizeStruct>(0);
    let mut io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.write_as(write, ZeroSizeStruct).unwrap();

    assert_eq!(caller_wrap.host_state_mut().memory.write_attempt_count(), 0);
}

#[test]
#[should_panic(expected = "buffer size is not equal to registered buffer size")]
fn test_write_with_zero_buffer_size() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);

    let mut registrar = MemoryAccessRegistrar::default();
    let write = registrar.register_write(0, 10);
    let mut io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.write(write, &[]).unwrap();
}

#[test]
fn test_write_of_some_size_buf() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    caller_wrap.host_state_mut().memory = MockMemory::new(1);

    let mut registrar = MemoryAccessRegistrar::default();
    let write = registrar.register_write(0, 10);
    let mut io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    let buffer = [0u8; 10];
    io.write(write, &buffer).unwrap();

    assert_eq!(caller_wrap.host_state_mut().memory.write_attempt_count(), 1);
}

#[test]
#[should_panic = "buffer size is not equal to registered buffer size"]
fn test_write_with_larger_buffer_size() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    caller_wrap.host_state_mut().memory = MockMemory::new(1);

    let mut registrar = MemoryAccessRegistrar::default();
    let write = registrar.register_write(0, 10);
    let mut io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    let buffer = [0u8; 20];
    io.write(write, &buffer).unwrap();
}

#[test]
fn test_write_as_with_zero_size_object() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    caller_wrap.host_state_mut().memory = MockMemory::new(1);

    let mut registrar = MemoryAccessRegistrar::default();
    let write = registrar.register_write_as::<u32>(0);
    let mut io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.write_as(write, 0).unwrap();
}

#[test]
fn test_write_as_with_same_object_size() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    caller_wrap.host_state_mut().memory = MockMemory::new(1);

    let mut registrar = MemoryAccessRegistrar::default();
    registrar.register_write_as::<u8>(0);
    let mut io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.write_as(
        WasmMemoryWriteAs {
            ptr: 0,
            _phantom: PhantomData,
        },
        1u8,
    )
    .unwrap();
}

#[test]
fn test_write_as_with_larger_object_size() {
    let mut store = new_store();
    let mut caller_wrap = CallerWrap::prepare(&mut store);
    caller_wrap.host_state_mut().memory = MockMemory::new(1);

    let mut registrar = MemoryAccessRegistrar::default();
    registrar.register_write_as::<u8>(0);
    let mut io: MemoryAccessIo = registrar.pre_process(&mut caller_wrap).unwrap();
    io.write_as(
        WasmMemoryWriteAs {
            ptr: WASM_PAGE_SIZE as u32,
            _phantom: PhantomData,
        },
        7u8,
    )
    .unwrap_err();
}

#[test]
fn test_register_read_of_valid_interval() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_read(0, 10);

    assert_eq!(result.ptr, 0);
    assert_eq!(result.size, 10);
    assert_eq!(registrar.reads.len(), 1);
    assert_eq!(registrar.writes.len(), 0);
}

#[test]
fn test_register_read_of_zero_size_buf() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_read(0, 0);

    assert_eq!(result.ptr, 0);
    assert_eq!(result.size, 0);
    assert_eq!(registrar.reads.len(), 0);
}

#[test]
fn test_register_read_of_zero_size_struct() {
    let mut mem_access_manager = MemoryAccessRegistrar::default();

    mem_access_manager.register_read_as::<ZeroSizeStruct>(142);

    assert_eq!(mem_access_manager.reads.len(), 0);
}

#[test]
fn test_register_read_of_zero_size_encoded_value() {
    let mut mem_access_manager = MemoryAccessRegistrar::default();

    mem_access_manager.register_read_decoded::<ZeroSizeStruct>(142);

    assert_eq!(mem_access_manager.reads.len(), 0);
}

#[test]
fn test_register_read_as_with_valid_interval() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_read_as::<u8>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(registrar.reads.len(), 1);
    assert_eq!(registrar.writes.len(), 0);
    assert_eq!(registrar.reads[0].offset, 0);
    assert_eq!(registrar.reads[0].size, core::mem::size_of::<u8>() as u32);
}

#[test]
fn test_register_read_as_with_zero_size() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_read_as::<u8>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(registrar.reads.len(), 1);
    assert_eq!(registrar.writes.len(), 0);
    assert_eq!(registrar.reads[0].offset, 0);
    assert_eq!(registrar.reads[0].size, core::mem::size_of::<u8>() as u32);
}

#[derive(Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[codec(crate = codec)]
struct TestStruct {
    a: u32,
    b: u64,
}

#[test]
fn test_register_read_decoded_with_valid_interval() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_read_decoded::<TestStruct>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(registrar.reads.len(), 1);
    assert_eq!(registrar.writes.len(), 0);
    assert_eq!(registrar.reads[0].offset, 0);
    assert_eq!(
        registrar.reads[0].size,
        TestStruct::max_encoded_len() as u32
    );
}

#[test]
fn test_register_read_decoded_with_zero_size() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_read_decoded::<TestStruct>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(registrar.reads.len(), 1);
    assert_eq!(registrar.writes.len(), 0);
    assert_eq!(registrar.reads[0].offset, 0);
    assert_eq!(
        registrar.reads[0].size,
        TestStruct::max_encoded_len() as u32
    );
}

#[test]
fn test_register_write_of_valid_interval() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_write(0, 10);

    assert_eq!(result.ptr, 0);
    assert_eq!(result.size, 10);
    assert_eq!(registrar.reads.len(), 0);
    assert_eq!(registrar.writes.len(), 1);
}

#[test]
fn test_register_write_of_zero_size_buf() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_write(0, 0);

    assert_eq!(result.ptr, 0);
    assert_eq!(result.size, 0);
    assert_eq!(registrar.reads.len(), 0);
    assert_eq!(registrar.writes.len(), 0);
}

#[test]
fn test_register_write_of_zero_size_struct() {
    let mut mem_access_manager = MemoryAccessRegistrar::default();

    mem_access_manager.register_write_as::<ZeroSizeStruct>(142);

    assert_eq!(mem_access_manager.writes.len(), 0);
}

#[test]
fn test_register_write_as_with_valid_interval() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_write_as::<u8>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(registrar.reads.len(), 0);
    assert_eq!(registrar.writes.len(), 1);
    assert_eq!(registrar.writes[0].offset, 0);
    assert_eq!(registrar.writes[0].size, core::mem::size_of::<u8>() as u32);
}

#[test]
fn test_register_write_as_with_zero_size() {
    let mut registrar = MemoryAccessRegistrar::default();

    let result = registrar.register_write_as::<u8>(0);

    assert_eq!(result.ptr, 0);
    assert_eq!(registrar.reads.len(), 0);
    assert_eq!(registrar.writes.len(), 1);
    assert_eq!(registrar.writes[0].offset, 0);
    assert_eq!(registrar.writes[0].size, core::mem::size_of::<u8>() as u32);
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
