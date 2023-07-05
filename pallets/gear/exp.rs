#[cfg(feature = "runtime-benchmarks")]
mod benchmarking {
    //! Benchmarks for the gear pallet
    //!
    //! ## i32const benchmarking
    //! Wasmer has many optimizations, that optimize i32const usage,
    //! so calculate this instruction constant weight is not easy.
    //! Because of this we suppose that i32const instruction has weight = 0,
    //! in cases we subtract its weight from benchmark weight to calculate
    //! benched instruction weight. But also we suppose i32const == i64const,
    //! when we calculate block code weight. This is more safe solution,
    //! but also more expensive.
    //!
    //! ## Drop, Block, End
    //! This is virtual instruction for wasmer, they aren't really generated in target code,
    //! the only thing they do - wasmer take them in account, when compiles wasm code.
    //! So, we suppose this instruction have weight 0.
    #![cfg(feature = "runtime-benchmarks")]
    #[allow(dead_code)]
    mod code {
        //! Functions to procedurally construct program code used for benchmarking.
        //!
        //! In order to be able to benchmark events that are triggered by program execution,
        //! we need to generate programs that perform those events.
        //! Because those programs can get very big we cannot simply define
        //! them as text (.wat) as this will be too slow and consume too much memory. Therefore
        //! we define this simple definition of a program that can be passed to `upload_code` that
        //! compiles it down into a `WasmModule` that can be used as a program's code.
        use crate::Config;
        use common::Origin;
        use frame_support::traits::Get;
        use gear_core::{ids::CodeId, memory::{PageU32Size, WasmPage}};
        use gear_wasm_instrument::{
            parity_wasm::{
                builder,
                elements::{
                    self, BlockType, CustomSection, FuncBody, Instruction, Instructions,
                    Section, ValueType,
                },
            },
            syscalls::SysCallName, STACK_END_EXPORT_NAME,
        };
        use sp_sandbox::{
            default_executor::{EnvironmentDefinitionBuilder, Memory},
            SandboxEnvironmentBuilder, SandboxMemory,
        };
        use sp_std::{borrow::ToOwned, convert::TryFrom, marker::PhantomData, prelude::*};
        /// The location where to put the generated code.
        pub enum Location {
            /// Generate all code into the `init` exported function.
            Init,
            /// Generate all code into the `handle` exported function.
            Handle,
        }
        /// Pass to `upload_code` in order to create a compiled `WasmModule`.
        ///
        /// This exists to have a more declarative way to describe a wasm module than to use
        /// parity-wasm directly. It is tailored to fit the structure of programs that are
        /// needed for benchmarking.
        pub struct ModuleDefinition {
            /// Imported memory attached to the module. No memory is imported if `None`.
            pub memory: Option<ImportedMemory>,
            /// Initializers for the imported memory.
            pub data_segments: Vec<DataSegment>,
            /// Creates the supplied amount of i64 mutable globals initialized with random values.
            pub num_globals: u32,
            /// List of syscalls that the module should import. They start with index 0.
            pub imported_functions: Vec<SysCallName>,
            /// Function body of the exported `init` function. Body is empty if `None`.
            /// Its index is `imported_functions.len()`.
            pub init_body: Option<FuncBody>,
            /// Function body of the exported `handle` function. Body is empty if `None`.
            /// Its index is `imported_functions.len() + 1`.
            pub handle_body: Option<FuncBody>,
            /// Function body of the exported `handle_reply` function. Body is empty if `None`.
            /// Its index is `imported_functions.len() + 2`.
            pub reply_body: Option<FuncBody>,
            /// Function body of the exported `handle_signal` function. Body is empty if `None`.
            /// Its index is `imported_functions.len() + 3`.
            pub signal_body: Option<FuncBody>,
            /// Function body of a non-exported function with index `imported_functions.len() + 4`.
            pub aux_body: Option<FuncBody>,
            /// The amount of I64 arguments the aux function should have.
            pub aux_arg_num: u32,
            pub aux_res: Option<ValueType>,
            /// Create a table containing function pointers.
            pub table: Option<TableSegment>,
            /// Create a section named "dummy" of the specified size. This is useful in order to
            /// benchmark the overhead of loading and storing codes of specified sizes. The dummy
            /// section only contributes to the size of the program but does not affect execution.
            pub dummy_section: u32,
            /// Create global export [STACK_END_GLOBAL_NAME] with the given page offset addr.
            /// If None, then all memory supposed to be stack memory, so stack end will be equal to memory size.
            pub stack_end: Option<WasmPage>,
        }
        #[automatically_derived]
        impl ::core::default::Default for ModuleDefinition {
            #[inline]
            fn default() -> ModuleDefinition {
                ModuleDefinition {
                    memory: ::core::default::Default::default(),
                    data_segments: ::core::default::Default::default(),
                    num_globals: ::core::default::Default::default(),
                    imported_functions: ::core::default::Default::default(),
                    init_body: ::core::default::Default::default(),
                    handle_body: ::core::default::Default::default(),
                    reply_body: ::core::default::Default::default(),
                    signal_body: ::core::default::Default::default(),
                    aux_body: ::core::default::Default::default(),
                    aux_arg_num: ::core::default::Default::default(),
                    aux_res: ::core::default::Default::default(),
                    table: ::core::default::Default::default(),
                    dummy_section: ::core::default::Default::default(),
                    stack_end: ::core::default::Default::default(),
                }
            }
        }
        pub struct TableSegment {
            /// How many elements should be created inside the table.
            pub num_elements: u32,
            /// The function index with which all table elements should be initialized.
            pub function_index: u32,
        }
        pub struct DataSegment {
            pub offset: u32,
            pub value: Vec<u8>,
        }
        pub struct ImportedMemory {
            pub min_pages: WasmPage,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for ImportedMemory {
            #[inline]
            fn clone(&self) -> ImportedMemory {
                ImportedMemory {
                    min_pages: ::core::clone::Clone::clone(&self.min_pages),
                }
            }
        }
        impl ImportedMemory {
            pub fn max<T: Config>() -> Self
            where
                T: Config,
            {
                Self {
                    min_pages: max_pages::<T>().into(),
                }
            }
            pub fn new(min_pages: u16) -> Self {
                Self {
                    min_pages: min_pages.into(),
                }
            }
        }
        pub struct ImportedFunction {
            pub module: &'static str,
            pub name: &'static str,
            pub params: Vec<ValueType>,
            pub return_type: Option<ValueType>,
        }
        /// A wasm module ready to be put on chain.
        pub struct WasmModule<T> {
            pub code: Vec<u8>,
            pub hash: CodeId,
            memory: Option<ImportedMemory>,
            _data: PhantomData<T>,
        }
        #[automatically_derived]
        impl<T: ::core::clone::Clone> ::core::clone::Clone for WasmModule<T> {
            #[inline]
            fn clone(&self) -> WasmModule<T> {
                WasmModule {
                    code: ::core::clone::Clone::clone(&self.code),
                    hash: ::core::clone::Clone::clone(&self.hash),
                    memory: ::core::clone::Clone::clone(&self.memory),
                    _data: ::core::clone::Clone::clone(&self._data),
                }
            }
        }
        pub const OFFSET_INIT: u32 = 0;
        pub const OFFSET_HANDLE: u32 = OFFSET_INIT + 1;
        pub const OFFSET_REPLY: u32 = OFFSET_HANDLE + 1;
        pub const OFFSET_SIGNAL: u32 = OFFSET_REPLY + 1;
        pub const OFFSET_AUX: u32 = OFFSET_SIGNAL + 1;
        impl<T: Config> From<ModuleDefinition> for WasmModule<T>
        where
            T: Config,
            T::AccountId: Origin,
        {
            fn from(def: ModuleDefinition) -> Self {
                let func_offset = u32::try_from(def.imported_functions.len()).unwrap();
                let mut program = builder::module()
                    .function()
                    .signature()
                    .build()
                    .with_body(def.init_body.unwrap_or_else(body::empty))
                    .build()
                    .function()
                    .signature()
                    .build()
                    .with_body(def.handle_body.unwrap_or_else(body::empty))
                    .build()
                    .function()
                    .signature()
                    .build()
                    .with_body(def.reply_body.unwrap_or_else(body::empty))
                    .build()
                    .function()
                    .signature()
                    .build()
                    .with_body(def.signal_body.unwrap_or_else(body::empty))
                    .build()
                    .export()
                    .field("init")
                    .internal()
                    .func(func_offset + OFFSET_INIT)
                    .build()
                    .export()
                    .field("handle")
                    .internal()
                    .func(func_offset + OFFSET_HANDLE)
                    .build()
                    .export()
                    .field("handle_reply")
                    .internal()
                    .func(func_offset + OFFSET_REPLY)
                    .build()
                    .export()
                    .field("handle_signal")
                    .internal()
                    .func(func_offset + OFFSET_SIGNAL)
                    .build();
                if let Some(body) = def.aux_body {
                    let mut signature = program.function().signature();
                    for _ in 0..def.aux_arg_num {
                        signature = signature.with_param(ValueType::I64);
                    }
                    if let Some(res) = def.aux_res {
                        signature = signature.with_result(res);
                    }
                    program = signature.build().with_body(body).build();
                }
                if let Some(memory) = &def.memory {
                    program = program
                        .import()
                        .module("env")
                        .field("memory")
                        .external()
                        .memory(memory.min_pages.raw(), None)
                        .build();
                }
                for name in def.imported_functions {
                    let sign = name.signature();
                    let sig = builder::signature()
                        .with_params(sign.params.into_iter().map(Into::into))
                        .with_results(sign.results.into_iter())
                        .build_sig();
                    let sig = program.push_signature(sig);
                    program = program
                        .import()
                        .module("env")
                        .field(name.to_str())
                        .with_external(elements::External::Function(sig))
                        .build();
                }
                for data in def.data_segments {
                    program = program
                        .data()
                        .offset(Instruction::I32Const(data.offset as i32))
                        .value(data.value)
                        .build();
                }
                if def.num_globals > 0 {
                    use rand::{distributions::Standard, prelude::*};
                    let rng = rand_pcg::Pcg32::seed_from_u64(3112244599778833558);
                    for val in rng.sample_iter(Standard).take(def.num_globals as usize) {
                        program = program
                            .global()
                            .value_type()
                            .i64()
                            .mutable()
                            .init_expr(Instruction::I64Const(val))
                            .build();
                    }
                }
                let stack_end = def
                    .stack_end
                    .unwrap_or(
                        def
                            .memory
                            .as_ref()
                            .map(|memory| memory.min_pages)
                            .unwrap_or(0.into()),
                    );
                program = program
                    .global()
                    .value_type()
                    .i32()
                    .init_expr(Instruction::I32Const(stack_end.offset() as i32))
                    .build()
                    .export()
                    .field(STACK_END_EXPORT_NAME)
                    .internal()
                    .global(def.num_globals)
                    .build();
                if let Some(table) = def.table {
                    program = program
                        .table()
                        .with_min(table.num_elements)
                        .with_max(Some(table.num_elements))
                        .with_element(
                            0,
                            ::alloc::vec::from_elem(
                                table.function_index,
                                table.num_elements as usize,
                            ),
                        )
                        .build();
                }
                if def.dummy_section > 0 {
                    program = program
                        .with_section(
                            Section::Custom(
                                CustomSection::new(
                                    "dummy".to_owned(),
                                    ::alloc::vec::from_elem(42, def.dummy_section as usize),
                                ),
                            ),
                        );
                }
                let code = program.build();
                let code = code.into_bytes().unwrap();
                let hash = CodeId::generate(&code);
                Self {
                    code: code.to_vec(),
                    hash,
                    memory: def.memory,
                    _data: PhantomData,
                }
            }
        }
        impl<T: Config> WasmModule<T>
        where
            T: Config,
            T::AccountId: Origin,
        {
            /// Creates a wasm module with an empty `handle` and `init` function and nothing else.
            pub fn dummy() -> Self {
                ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    ..Default::default()
                }
                    .into()
            }
            /// Creates a wasm module of `target_bytes` size. The generated module maximizes
            /// instrumentation runtime by nesting blocks as deeply as possible given the byte budget.
            /// `code_location`: Whether to place the code into `init` or `handle`.
            pub fn sized(target_bytes: u32, code_location: Location) -> Self {
                use self::elements::Instruction::{
                    Drop, End, I32Const, I64Const, I64Eq, If, Return,
                };
                let expansions = (target_bytes.saturating_sub(63) / 20)
                    .saturating_sub(1);
                const EXPANSION: &[Instruction] = &[
                    I64Const(0),
                    I64Const(1),
                    I64Eq,
                    If(BlockType::NoResult),
                    Return,
                    End,
                    I32Const(0xffffff),
                    Drop,
                    I32Const(0),
                    If(BlockType::NoResult),
                    Return,
                    End,
                ];
                let mut module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    ..Default::default()
                };
                let body = Some(body::repeated(expansions, EXPANSION));
                match code_location {
                    Location::Init => module.init_body = body,
                    Location::Handle => module.handle_body = body,
                }
                module.into()
            }
            /// Creates a memory instance for use in a sandbox with dimensions declared in this module
            /// and adds it to `env`. A reference to that memory is returned so that it can be used to
            /// access the memory contents from the supervisor.
            pub fn add_memory<S>(
                &self,
                env: &mut EnvironmentDefinitionBuilder<S>,
            ) -> Option<Memory> {
                let memory = if let Some(memory) = &self.memory {
                    memory
                } else {
                    return None;
                };
                let memory = Memory::new(memory.min_pages.raw(), None).unwrap();
                env.add_memory("env", "memory", memory.clone());
                Some(memory)
            }
            pub fn unary_instr_64(instr: Instruction, repeat: u32) -> Self {
                Self::unary_instr_for_bit_width(instr, BitWidth::X64, repeat)
            }
            pub fn unary_instr_32(instr: Instruction, repeat: u32) -> Self {
                Self::unary_instr_for_bit_width(instr, BitWidth::X86, repeat)
            }
            fn unary_instr_for_bit_width(
                instr: Instruction,
                bit_width: BitWidth,
                repeat: u32,
            ) -> Self {
                use body::DynInstr::Regular;
                ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            repeat,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    bit_width.random_repeated(1),
                                    Regular(instr),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }
                    .into()
            }
            pub fn binary_instr_64(instr: Instruction, repeat: u32) -> Self {
                Self::binary_instr_for_bit_width(instr, BitWidth::X64, repeat)
            }
            pub fn binary_instr_32(instr: Instruction, repeat: u32) -> Self {
                Self::binary_instr_for_bit_width(instr, BitWidth::X86, repeat)
            }
            fn binary_instr_for_bit_width(
                instr: Instruction,
                bit_width: BitWidth,
                repeat: u32,
            ) -> Self {
                use body::DynInstr::Regular;
                ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            repeat,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    bit_width.random_repeated(2),
                                    Regular(instr),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }
                    .into()
            }
        }
        /// Mechanisms to generate a function body that can be used inside a `ModuleDefinition`.
        pub mod body {
            use gear_core::memory::{GearPage, PageU32Size, WasmPage};
            use super::*;
            /// When generating contract code by repeating a wasm sequence, it's sometimes necessary
            /// to change those instructions on each repetition. The variants of this enum describe
            /// various ways in which this can happen.
            pub enum DynInstr {
                /// Insert `i32.const (self.0 as i32)` operation
                InstrI32Const(u32),
                /// Insert `i64.const (self.0 as i64)` operation
                InstrI64Const(u64),
                /// Insert `call self.0` operation
                InstrCall(u32),
                /// Insert `i32.load align=self.0, offset=self.1` operation
                InstrI32Load(u32, u32),
                /// Insert the associated instruction.
                Regular(Instruction),
                /// Insert a I32Const with incrementing value for each insertion.
                /// (start_at, increment_by)
                Counter(u32, u32),
                /// Insert a I32Const with a random value in [low, high) not divisible by two.
                /// (low, high)
                RandomUnaligned(u32, u32),
                /// Insert a I32Const with a random value in [low, high).
                /// (low, high)
                RandomI32(i32, i32),
                /// Insert a I64Const with a random value in [low, high).
                /// (low, high)
                RandomI64(i64, i64),
                /// Insert the specified amount of I32Const with a random value.
                RandomI32Repeated(usize),
                /// Insert the specified amount of I64Const with a random value.
                RandomI64Repeated(usize),
                /// Insert a GetLocal with a random offset in [low, high).
                /// (low, high)
                RandomGetLocal(u32, u32),
                /// Insert a SetLocal with a random offset in [low, high).
                /// (low, high)
                RandomSetLocal(u32, u32),
                /// Insert a TeeLocal with a random offset in [low, high).
                /// (low, high)
                RandomTeeLocal(u32, u32),
                /// Insert a GetGlobal with a random offset in [low, high).
                /// (low, high)
                RandomGetGlobal(u32, u32),
                /// Insert a SetGlobal with a random offset in [low, high).
                /// (low, high)
                RandomSetGlobal(u32, u32),
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for DynInstr {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        DynInstr::InstrI32Const(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "InstrI32Const",
                                &__self_0,
                            )
                        }
                        DynInstr::InstrI64Const(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "InstrI64Const",
                                &__self_0,
                            )
                        }
                        DynInstr::InstrCall(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "InstrCall",
                                &__self_0,
                            )
                        }
                        DynInstr::InstrI32Load(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "InstrI32Load",
                                __self_0,
                                &__self_1,
                            )
                        }
                        DynInstr::Regular(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Regular",
                                &__self_0,
                            )
                        }
                        DynInstr::Counter(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "Counter",
                                __self_0,
                                &__self_1,
                            )
                        }
                        DynInstr::RandomUnaligned(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "RandomUnaligned",
                                __self_0,
                                &__self_1,
                            )
                        }
                        DynInstr::RandomI32(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "RandomI32",
                                __self_0,
                                &__self_1,
                            )
                        }
                        DynInstr::RandomI64(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "RandomI64",
                                __self_0,
                                &__self_1,
                            )
                        }
                        DynInstr::RandomI32Repeated(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "RandomI32Repeated",
                                &__self_0,
                            )
                        }
                        DynInstr::RandomI64Repeated(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "RandomI64Repeated",
                                &__self_0,
                            )
                        }
                        DynInstr::RandomGetLocal(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "RandomGetLocal",
                                __self_0,
                                &__self_1,
                            )
                        }
                        DynInstr::RandomSetLocal(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "RandomSetLocal",
                                __self_0,
                                &__self_1,
                            )
                        }
                        DynInstr::RandomTeeLocal(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "RandomTeeLocal",
                                __self_0,
                                &__self_1,
                            )
                        }
                        DynInstr::RandomGetGlobal(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "RandomGetGlobal",
                                __self_0,
                                &__self_1,
                            )
                        }
                        DynInstr::RandomSetGlobal(__self_0, __self_1) => {
                            ::core::fmt::Formatter::debug_tuple_field2_finish(
                                f,
                                "RandomSetGlobal",
                                __self_0,
                                &__self_1,
                            )
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::clone::Clone for DynInstr {
                #[inline]
                fn clone(&self) -> DynInstr {
                    match self {
                        DynInstr::InstrI32Const(__self_0) => {
                            DynInstr::InstrI32Const(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        DynInstr::InstrI64Const(__self_0) => {
                            DynInstr::InstrI64Const(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        DynInstr::InstrCall(__self_0) => {
                            DynInstr::InstrCall(::core::clone::Clone::clone(__self_0))
                        }
                        DynInstr::InstrI32Load(__self_0, __self_1) => {
                            DynInstr::InstrI32Load(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        DynInstr::Regular(__self_0) => {
                            DynInstr::Regular(::core::clone::Clone::clone(__self_0))
                        }
                        DynInstr::Counter(__self_0, __self_1) => {
                            DynInstr::Counter(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        DynInstr::RandomUnaligned(__self_0, __self_1) => {
                            DynInstr::RandomUnaligned(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        DynInstr::RandomI32(__self_0, __self_1) => {
                            DynInstr::RandomI32(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        DynInstr::RandomI64(__self_0, __self_1) => {
                            DynInstr::RandomI64(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        DynInstr::RandomI32Repeated(__self_0) => {
                            DynInstr::RandomI32Repeated(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        DynInstr::RandomI64Repeated(__self_0) => {
                            DynInstr::RandomI64Repeated(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        DynInstr::RandomGetLocal(__self_0, __self_1) => {
                            DynInstr::RandomGetLocal(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        DynInstr::RandomSetLocal(__self_0, __self_1) => {
                            DynInstr::RandomSetLocal(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        DynInstr::RandomTeeLocal(__self_0, __self_1) => {
                            DynInstr::RandomTeeLocal(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        DynInstr::RandomGetGlobal(__self_0, __self_1) => {
                            DynInstr::RandomGetGlobal(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                        DynInstr::RandomSetGlobal(__self_0, __self_1) => {
                            DynInstr::RandomSetGlobal(
                                ::core::clone::Clone::clone(__self_0),
                                ::core::clone::Clone::clone(__self_1),
                            )
                        }
                    }
                }
            }
            pub fn write_access_all_pages_instrs(
                mem_size: WasmPage,
                mut head: Vec<Instruction>,
            ) -> Vec<Instruction> {
                for page in mem_size
                    .iter_from_zero()
                    .flat_map(|p| p.to_pages_iter::<GearPage>())
                {
                    head.push(Instruction::I32Const(page.offset() as i32));
                    head.push(Instruction::I32Const(42));
                    head.push(Instruction::I32Store(2, 0));
                }
                head
            }
            pub fn read_access_all_pages_instrs(
                mem_size: WasmPage,
                mut head: Vec<Instruction>,
            ) -> Vec<Instruction> {
                for page in mem_size
                    .iter_from_zero()
                    .flat_map(|p| p.to_pages_iter::<GearPage>())
                {
                    head.push(Instruction::I32Const(page.offset() as i32));
                    head.push(Instruction::I32Load(2, 0));
                    head.push(Instruction::Drop);
                }
                head
            }
            pub fn repeated_dyn_instr(
                repetitions: u32,
                mut instructions: Vec<DynInstr>,
                mut head: Vec<Instruction>,
            ) -> Vec<Instruction> {
                use rand::{distributions::Standard, prelude::*};
                let mut rng = rand_pcg::Pcg32::seed_from_u64(8446744073709551615);
                let instr_iter = (0..instructions.len())
                    .cycle()
                    .take(instructions.len() * usize::try_from(repetitions).unwrap())
                    .flat_map(|idx| match &mut instructions[idx] {
                        DynInstr::InstrI32Const(c) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([Instruction::I32Const(*c as i32)]),
                            )
                        }
                        DynInstr::InstrI64Const(c) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([Instruction::I64Const(*c as i64)]),
                            )
                        }
                        DynInstr::InstrCall(c) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([Instruction::Call(*c)]),
                            )
                        }
                        DynInstr::InstrI32Load(align, offset) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::I32Load(*align, *offset),
                                ]),
                            )
                        }
                        DynInstr::Regular(instruction) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([instruction.clone()]),
                            )
                        }
                        DynInstr::Counter(offset, increment_by) => {
                            let current = *offset;
                            *offset += *increment_by;
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::I32Const(current as i32),
                                ]),
                            )
                        }
                        DynInstr::RandomUnaligned(low, high) => {
                            let unaligned = rng.gen_range(*low..*high) | 1;
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::I32Const(unaligned as i32),
                                ]),
                            )
                        }
                        DynInstr::RandomI32(low, high) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::I32Const(rng.gen_range(*low..*high)),
                                ]),
                            )
                        }
                        DynInstr::RandomI64(low, high) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::I64Const(rng.gen_range(*low..*high)),
                                ]),
                            )
                        }
                        DynInstr::RandomI32Repeated(num) => {
                            (&mut rng)
                                .sample_iter(Standard)
                                .take(*num)
                                .map(Instruction::I32Const)
                                .collect()
                        }
                        DynInstr::RandomI64Repeated(num) => {
                            (&mut rng)
                                .sample_iter(Standard)
                                .take(*num)
                                .map(Instruction::I64Const)
                                .collect()
                        }
                        DynInstr::RandomGetLocal(low, high) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::GetLocal(rng.gen_range(*low..*high)),
                                ]),
                            )
                        }
                        DynInstr::RandomSetLocal(low, high) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::SetLocal(rng.gen_range(*low..*high)),
                                ]),
                            )
                        }
                        DynInstr::RandomTeeLocal(low, high) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::TeeLocal(rng.gen_range(*low..*high)),
                                ]),
                            )
                        }
                        DynInstr::RandomGetGlobal(low, high) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::GetGlobal(rng.gen_range(*low..*high)),
                                ]),
                            )
                        }
                        DynInstr::RandomSetGlobal(low, high) => {
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::SetGlobal(rng.gen_range(*low..*high)),
                                ]),
                            )
                        }
                    });
                head.extend(instr_iter);
                head
            }
            pub fn to_dyn(instructions: &[Instruction]) -> Vec<DynInstr> {
                instructions.iter().cloned().map(DynInstr::Regular).collect()
            }
            pub fn with_result_check_dyn(
                res_offset: DynInstr,
                instructions: &[DynInstr],
            ) -> Vec<DynInstr> {
                let mut res = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        DynInstr::Regular(Instruction::Block(BlockType::NoResult)),
                        res_offset,
                        DynInstr::InstrI32Load(2, 0),
                        DynInstr::Regular(Instruction::I32Eqz),
                        DynInstr::Regular(Instruction::BrIf(0)),
                        DynInstr::Regular(Instruction::Unreachable),
                        DynInstr::Regular(Instruction::End),
                    ]),
                );
                res.splice(1..1, instructions.iter().cloned());
                res
            }
            pub fn fallible_syscall_instr(
                repetitions: u32,
                call_index: u32,
                res_offset: DynInstr,
                params: &[DynInstr],
            ) -> Vec<Instruction> {
                let mut instructions = params.to_vec();
                instructions
                    .extend([res_offset.clone(), DynInstr::InstrCall(call_index)]);
                if false {
                    instructions = with_result_check_dyn(res_offset, &instructions);
                }
                repeated_dyn_instr(repetitions, instructions, ::alloc::vec::Vec::new())
            }
            pub fn from_instructions(mut instructions: Vec<Instruction>) -> FuncBody {
                instructions.push(Instruction::End);
                FuncBody::new(::alloc::vec::Vec::new(), Instructions::new(instructions))
            }
            pub fn empty() -> FuncBody {
                FuncBody::new(::alloc::vec::Vec::new(), Instructions::empty())
            }
            pub fn repeated(repetitions: u32, instructions: &[Instruction]) -> FuncBody {
                let instructions = instructions
                    .iter()
                    .cycle()
                    .take(instructions.len() * usize::try_from(repetitions).unwrap())
                    .cloned()
                    .collect();
                from_instructions(instructions)
            }
            pub fn repeated_dyn(
                repetitions: u32,
                instructions: Vec<DynInstr>,
            ) -> FuncBody {
                let instructions = repeated_dyn_instr(
                    repetitions,
                    instructions,
                    ::alloc::vec::Vec::new(),
                );
                from_instructions(instructions)
            }
            pub fn fallible_syscall(
                repetitions: u32,
                res_offset: u32,
                params: &[DynInstr],
            ) -> FuncBody {
                let mut instructions = params.to_vec();
                instructions
                    .extend([
                        DynInstr::InstrI32Const(res_offset),
                        DynInstr::InstrCall(0),
                    ]);
                if false {
                    instructions = with_result_check_dyn(
                        DynInstr::InstrI32Const(res_offset),
                        &instructions,
                    );
                }
                repeated_dyn(repetitions, instructions)
            }
            pub fn syscall(repetitions: u32, params: &[DynInstr]) -> FuncBody {
                let mut instructions = params.to_vec();
                instructions.push(DynInstr::InstrCall(0));
                repeated_dyn(repetitions, instructions)
            }
            pub fn prepend(body: &mut FuncBody, instructions: Vec<Instruction>) {
                body.code_mut()
                    .elements_mut()
                    .splice(0..0, instructions.iter().cloned());
            }
            /// Replace the locals of the supplied `body` with `num` i64 locals.
            pub fn inject_locals(body: &mut FuncBody, num: u32) {
                use self::elements::Local;
                *body
                    .locals_mut() = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([Local::new(num, ValueType::I64)]),
                );
            }
            pub fn unreachable_condition(
                instructions: &mut Vec<Instruction>,
                flag: Instruction,
            ) {
                let additional = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        flag,
                        Instruction::If(BlockType::NoResult),
                        Instruction::Unreachable,
                        Instruction::End,
                    ]),
                );
                instructions.extend(additional)
            }
        }
        /// The maximum amount of pages any program is allowed to have according to the current `Schedule`.
        pub fn max_pages<T: Config>() -> u16
        where
            T: Config,
        {
            T::Schedule::get().limits.memory_pages
        }
        enum BitWidth {
            X64,
            X86,
        }
        impl BitWidth {
            pub fn random_repeated(&self, count: usize) -> body::DynInstr {
                match self {
                    BitWidth::X64 => body::DynInstr::RandomI64Repeated(count),
                    BitWidth::X86 => body::DynInstr::RandomI32Repeated(count),
                }
            }
        }
    }
    mod sandbox {
        /// ! For instruction benchmarking we do no instantiate a full program but merely the
        /// ! sandbox to execute the wasm code. This is because we do not need the full
        /// ! environment that provides the seal interface as imported functions.
        use super::{
            code::{ModuleDefinition, WasmModule},
            Config,
        };
        use common::Origin;
        use sp_sandbox::{
            default_executor::{EnvironmentDefinitionBuilder, Instance, Memory},
            SandboxEnvironmentBuilder, SandboxInstance,
        };
        /// Minimal execution environment without any exported functions.
        pub struct Sandbox {
            instance: Instance<()>,
            _memory: Option<Memory>,
        }
        impl Sandbox {
            /// Invoke the `handle` function of a program code and panic on any execution error.
            pub fn invoke(&mut self) {
                self.instance.invoke("handle", &[], &mut ()).unwrap();
            }
        }
        impl<T: Config> From<&WasmModule<T>> for Sandbox
        where
            T: Config,
            T::AccountId: Origin,
        {
            /// Creates an instance from the supplied module and supplies as much memory
            /// to the instance as the module declares as imported.
            fn from(module: &WasmModule<T>) -> Self {
                let mut env_builder = EnvironmentDefinitionBuilder::new();
                let memory = module.add_memory(&mut env_builder);
                let instance = Instance::new(&module.code, &env_builder, &mut ())
                    .expect("Failed to create benchmarking Sandbox instance");
                Self { instance, _memory: memory }
            }
        }
        impl Sandbox {
            /// Creates an instance from the supplied module and supplies as much memory
            /// to the instance as the module declares as imported.
            pub fn from_module_def<T>(module: ModuleDefinition) -> Self
            where
                T: Config,
                T::AccountId: Origin,
            {
                let module: WasmModule<T> = module.into();
                let mut env_builder = EnvironmentDefinitionBuilder::new();
                let memory = module.add_memory(&mut env_builder);
                let instance = Instance::new(&module.code, &env_builder, &mut ())
                    .expect("Failed to create benchmarking Sandbox instance");
                Self { instance, _memory: memory }
            }
        }
    }
    mod syscalls {
        //! Benchmarks for gear sys-calls.
        use super::{
            code::{
                body::{self, unreachable_condition, DynInstr::*},
                max_pages, DataSegment, ImportedMemory, ModuleDefinition, WasmModule,
            },
            utils::{self, PrepareConfig},
            Exec, Program, API_BENCHMARK_BATCHES,
        };
        use crate::{
            benchmarking::MAX_PAYLOAD_LEN, manager::HandleKind,
            schedule::{ALLOC_BENCHMARK_BATCH_SIZE, API_BENCHMARK_BATCH_SIZE},
            Config, MailboxOf, Pallet as Gear, ProgramStorageOf,
        };
        use alloc::{vec, vec::Vec};
        use common::{benchmarking, storage::*, Origin, ProgramStorage};
        use core::{marker::PhantomData, mem::size_of};
        use frame_system::RawOrigin;
        use gear_core::{
            ids::{CodeId, MessageId, ProgramId, ReservationId},
            memory::{GearPage, PageBuf, PageBufInner, PageU32Size, WasmPage},
            message::{Message, Value},
            reservation::GasReservationSlot,
        };
        use gear_wasm_instrument::{
            parity_wasm::elements::Instruction, syscalls::SysCallName,
        };
        use sp_core::Get;
        use sp_runtime::{codec::Encode, traits::UniqueSaturatedInto};
        /// Size of fallible syscall error length
        const ERR_LEN_SIZE: u32 = size_of::<u32>() as u32;
        /// Handle size
        const HANDLE_SIZE: u32 = size_of::<u32>() as u32;
        /// Value size
        const VALUE_SIZE: u32 = size_of::<Value>() as u32;
        /// Reservation id size
        const RID_SIZE: u32 = size_of::<ReservationId>() as u32;
        /// Code id size
        const CID_SIZE: u32 = size_of::<CodeId>() as u32;
        /// Program id size
        const PID_SIZE: u32 = size_of::<ProgramId>() as u32;
        /// Message id size
        const MID_SIZE: u32 = size_of::<MessageId>() as u32;
        /// Random subject size
        const RANDOM_SUBJECT_SIZE: u32 = 32;
        /// Size of struct with fields: error len and handle
        const ERR_HANDLE_SIZE: u32 = ERR_LEN_SIZE + HANDLE_SIZE;
        /// Size of struct with fields: error len and message id
        const ERR_MID_SIZE: u32 = ERR_LEN_SIZE + MID_SIZE;
        /// Size of struct with fields: reservation id and value
        const RID_VALUE_SIZE: u32 = RID_SIZE + VALUE_SIZE;
        /// Size of struct with fields: program id and value
        const PID_VALUE_SIZE: u32 = PID_SIZE + VALUE_SIZE;
        /// Size of struct with fields: code id and value
        const CID_VALUE_SIZE: u32 = CID_SIZE + VALUE_SIZE;
        /// Size of struct with fields: reservation id, program id and value
        const RID_PID_VALUE_SIZE: u32 = RID_SIZE + PID_SIZE + VALUE_SIZE;
        /// Size of memory with one wasm page
        const SMALL_MEM_SIZE: u16 = 1;
        /// Common offset for data in memory. We use `1` to make memory accesses unaligned
        /// and therefore slower, because we wanna to identify max weights.
        const COMMON_OFFSET: u32 = 1;
        /// Common small payload len.
        const COMMON_PAYLOAD_LEN: u32 = 100;
        const MAX_REPETITIONS: u32 = API_BENCHMARK_BATCHES * API_BENCHMARK_BATCH_SIZE;
        fn kb_to_bytes(size_in_kb: u32) -> u32 {
            size_in_kb.checked_mul(1024).unwrap()
        }
        pub(crate) struct Benches<T>
        where
            T: Config,
            T::AccountId: Origin,
        {
            _phantom: PhantomData<T>,
        }
        impl<T> Benches<T>
        where
            T: Config,
            T::AccountId: Origin,
        {
            fn prepare_handle(
                module: ModuleDefinition,
                value: u32,
            ) -> Result<Exec<T>, &'static str> {
                let instance = Program::<
                    T,
                >::new(module.into(), ::alloc::vec::Vec::new())?;
                utils::prepare_exec::<
                    T,
                >(
                    instance.caller.into_origin(),
                    HandleKind::Handle(ProgramId::from_origin(instance.addr)),
                    ::alloc::vec::Vec::new(),
                    PrepareConfig {
                        value: value.into(),
                        ..Default::default()
                    },
                )
            }
            fn prepare_handle_with_reservation_slots(
                module: ModuleDefinition,
                repetitions: u32,
            ) -> Result<Exec<T>, &'static str> {
                let instance = Program::<
                    T,
                >::new(module.into(), ::alloc::vec::Vec::new())?;
                let program_id = ProgramId::from_origin(instance.addr);
                ProgramStorageOf::<
                    T,
                >::update_active_program(
                        program_id,
                        |program| {
                            for x in 0..repetitions {
                                program
                                    .gas_reservation_map
                                    .insert(
                                        ReservationId::from(x as u64),
                                        GasReservationSlot {
                                            amount: 1_000,
                                            start: 1,
                                            finish: 100,
                                        },
                                    );
                            }
                        },
                    )
                    .unwrap();
                utils::prepare_exec::<
                    T,
                >(
                    instance.caller.into_origin(),
                    HandleKind::Handle(program_id),
                    ::alloc::vec::Vec::new(),
                    Default::default(),
                )
            }
            fn prepare_handle_with_const_payload(
                module: ModuleDefinition,
            ) -> Result<Exec<T>, &'static str> {
                let instance = Program::<
                    T,
                >::new(module.into(), ::alloc::vec::Vec::new())?;
                utils::prepare_exec::<
                    T,
                >(
                    instance.caller.into_origin(),
                    HandleKind::Handle(ProgramId::from_origin(instance.addr)),
                    ::alloc::vec::from_elem(0xff, MAX_PAYLOAD_LEN as usize),
                    Default::default(),
                )
            }
            pub fn alloc(repetitions: u32, pages: u32) -> Result<Exec<T>, &'static str> {
                if !(repetitions * pages * ALLOC_BENCHMARK_BATCH_SIZE
                    < max_pages::<T>() as u32)
                {
                    ::core::panicking::panic(
                        "assertion failed: repetitions * pages * ALLOC_BENCHMARK_BATCH_SIZE < max_pages::<T>() as u32",
                    )
                }
                let mut instructions = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        Instruction::I32Const(pages as i32),
                        Instruction::Call(0),
                        Instruction::I32Const(-1),
                    ]),
                );
                unreachable_condition(&mut instructions, Instruction::I32Eq);
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(0)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Alloc]),
                    ),
                    handle_body: Some(
                        body::repeated(
                            repetitions * ALLOC_BENCHMARK_BATCH_SIZE,
                            &instructions,
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn free(r: u32) -> Result<Exec<T>, &'static str> {
                if !(r <= max_pages::<T>() as u32) {
                    ::core::panicking::panic(
                        "assertion failed: r <= max_pages::<T>() as u32",
                    )
                }
                use Instruction::*;
                let mut instructions = ::alloc::vec::Vec::new();
                for _ in 0..API_BENCHMARK_BATCH_SIZE {
                    instructions.extend([I32Const(r as i32), Call(0), I32Const(-1)]);
                    unreachable_condition(&mut instructions, I32Eq);
                    for page in 0..r {
                        instructions
                            .extend([I32Const(page as i32), Call(1), I32Const(0)]);
                        unreachable_condition(&mut instructions, I32Ne);
                    }
                }
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(0)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Alloc, SysCallName::Free]),
                    ),
                    init_body: None,
                    handle_body: Some(body::from_instructions(instructions)),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r;
                let res_offset = COMMON_OFFSET;
                let mailbox_threshold = <T as Config>::MailboxThreshold::get();
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::ReserveGas]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[InstrI64Const(mailbox_threshold), InstrI32Const(1)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_unreserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r;
                if !(repetitions <= MAX_REPETITIONS) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= MAX_REPETITIONS",
                    )
                }
                let reservation_id_bytes: Vec<u8> = (0..MAX_REPETITIONS)
                    .map(|i| ReservationId::from(i as u64))
                    .flat_map(|x| x.encode())
                    .collect();
                let reservation_id_offset = COMMON_OFFSET;
                let res_offset = reservation_id_offset
                    + reservation_id_bytes.len() as u32;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::UnreserveGas]),
                    ),
                    data_segments: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            DataSegment {
                                offset: reservation_id_offset,
                                value: reservation_id_bytes,
                            },
                        ]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[Counter(reservation_id_offset, RID_SIZE)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_reservation_slots(module, repetitions)
            }
            pub fn gr_system_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let res_offset = COMMON_OFFSET;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::SystemReserveGas]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[InstrI64Const(50_000_000)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn getter(name: SysCallName, r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let res_offset = COMMON_OFFSET;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([name]),
                    ),
                    handle_body: Some(
                        body::syscall(repetitions, &[InstrI32Const(res_offset)]),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_read(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let buffer_offset = COMMON_OFFSET;
                let buffer_len = COMMON_PAYLOAD_LEN;
                let res_offset = buffer_offset + buffer_len;
                if !(buffer_len <= MAX_PAYLOAD_LEN) {
                    ::core::panicking::panic(
                        "assertion failed: buffer_len <= MAX_PAYLOAD_LEN",
                    )
                }
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Read]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[
                                InstrI32Const(0),
                                InstrI32Const(buffer_len),
                                InstrI32Const(buffer_offset),
                            ],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_const_payload(module)
            }
            pub fn gr_read_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = API_BENCHMARK_BATCH_SIZE;
                let buffer_offset = COMMON_OFFSET;
                let buffer_len = n * 1024;
                let res_offset = buffer_offset + buffer_len;
                if !(buffer_len <= MAX_PAYLOAD_LEN) {
                    ::core::panicking::panic(
                        "assertion failed: buffer_len <= MAX_PAYLOAD_LEN",
                    )
                }
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Read]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[
                                InstrI32Const(0),
                                InstrI32Const(buffer_len),
                                InstrI32Const(buffer_offset),
                            ],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_const_payload(module)
            }
            pub fn gr_random(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let subject_offset = COMMON_OFFSET;
                let bn_random_offset = subject_offset + RANDOM_SUBJECT_SIZE;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Random]),
                    ),
                    handle_body: Some(
                        body::syscall(
                            repetitions,
                            &[
                                InstrI32Const(subject_offset),
                                InstrI32Const(bn_random_offset),
                            ],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_reply_deposit(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let pid_value_offset = COMMON_OFFSET;
                let send_res_offset = COMMON_OFFSET + PID_VALUE_SIZE;
                let mid_offset = send_res_offset + ERR_LEN_SIZE;
                let res_offset = send_res_offset + ERR_MID_SIZE;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            SysCallName::ReplyDeposit,
                            SysCallName::Send,
                        ]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[
                                InstrI32Const(pid_value_offset),
                                InstrI32Const(COMMON_OFFSET),
                                InstrI32Const(0),
                                InstrI32Const(0),
                                InstrI32Const(send_res_offset),
                                InstrCall(1),
                                InstrI32Const(mid_offset),
                                InstrI64Const(10_000),
                            ],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 10000000)
            }
            pub fn gr_send(
                batches: u32,
                payload_len_kb: Option<u32>,
                wgas: bool,
            ) -> Result<Exec<T>, &'static str> {
                let repetitions = batches * API_BENCHMARK_BATCH_SIZE;
                let pid_value_offset = COMMON_OFFSET;
                let payload_offset = pid_value_offset + PID_VALUE_SIZE;
                let payload_len = payload_len_kb
                    .map(kb_to_bytes)
                    .unwrap_or(COMMON_PAYLOAD_LEN);
                let res_offset = payload_offset + payload_len;
                let mut params = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        InstrI32Const(pid_value_offset),
                        InstrI32Const(payload_offset),
                        InstrI32Const(payload_len),
                        InstrI32Const(10),
                    ]),
                );
                let name = if wgas {
                    params.insert(3, InstrI64Const(100_000_000));
                    SysCallName::SendWGas
                } else {
                    SysCallName::Send
                };
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([name]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(repetitions, res_offset, &params),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 10000000)
            }
            pub fn gr_send_init(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let res_offset = COMMON_OFFSET;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::SendInit]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(repetitions, res_offset, &[]),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_send_push(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                if !(repetitions <= MAX_REPETITIONS) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= MAX_REPETITIONS",
                    )
                }
                let payload_offset = COMMON_OFFSET;
                let payload_len = COMMON_PAYLOAD_LEN;
                let res_offset = payload_offset + payload_len;
                let err_handle_offset = res_offset + ERR_LEN_SIZE;
                let mut instructions = body::fallible_syscall_instr(
                    MAX_REPETITIONS,
                    1,
                    Counter(err_handle_offset, ERR_HANDLE_SIZE),
                    &[],
                );
                instructions
                    .extend(
                        body::fallible_syscall_instr(
                            repetitions,
                            0,
                            InstrI32Const(res_offset),
                            &[
                                Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
                                InstrI32Load(2, 0),
                                InstrI32Const(payload_offset),
                                InstrI32Const(payload_len),
                            ],
                        ),
                    );
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            SysCallName::SendPush,
                            SysCallName::SendInit,
                        ]),
                    ),
                    handle_body: Some(body::from_instructions(instructions)),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_send_push_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = API_BENCHMARK_BATCH_SIZE;
                let payload_offset = COMMON_OFFSET;
                let payload_len = n * 1024;
                let res_offset = payload_offset + payload_len;
                let err_handle_offset = res_offset + ERR_LEN_SIZE;
                let mut instructions = body::fallible_syscall_instr(
                    API_BENCHMARK_BATCH_SIZE,
                    1,
                    Counter(err_handle_offset, ERR_HANDLE_SIZE),
                    &[],
                );
                instructions
                    .extend(
                        body::fallible_syscall_instr(
                            repetitions,
                            0,
                            InstrI32Const(res_offset),
                            &[
                                Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
                                InstrI32Load(2, 0),
                                InstrI32Const(payload_offset),
                                InstrI32Const(payload_len),
                            ],
                        ),
                    );
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            SysCallName::SendPush,
                            SysCallName::SendInit,
                        ]),
                    ),
                    handle_body: Some(body::from_instructions(instructions)),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_send_commit(r: u32, wgas: bool) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                if !(repetitions <= MAX_REPETITIONS) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= MAX_REPETITIONS",
                    )
                }
                let pid_value_offset = COMMON_OFFSET;
                let err_handle_offset = pid_value_offset + PID_VALUE_SIZE;
                let res_offset = err_handle_offset + MAX_REPETITIONS * ERR_HANDLE_SIZE;
                let mut instructions = body::fallible_syscall_instr(
                    MAX_REPETITIONS,
                    1,
                    Counter(err_handle_offset, ERR_HANDLE_SIZE),
                    &[],
                );
                let mut commit_params = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
                        InstrI32Load(2, 0),
                        InstrI32Const(pid_value_offset),
                        InstrI32Const(10),
                    ]),
                );
                let name = if wgas {
                    commit_params.insert(3, InstrI64Const(100_000_000));
                    SysCallName::SendCommitWGas
                } else {
                    SysCallName::SendCommit
                };
                instructions
                    .extend(
                        body::fallible_syscall_instr(
                            repetitions,
                            0,
                            InstrI32Const(res_offset),
                            &commit_params,
                        ),
                    );
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([name, SysCallName::SendInit]),
                    ),
                    handle_body: Some(body::from_instructions(instructions)),
                    ..Default::default()
                };
                Self::prepare_handle(module, 10000000)
            }
            pub fn gr_reservation_send(
                batches: u32,
                payload_len_kb: Option<u32>,
            ) -> Result<Exec<T>, &'static str> {
                let repetitions = batches * API_BENCHMARK_BATCH_SIZE;
                if !(repetitions <= MAX_REPETITIONS) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= MAX_REPETITIONS",
                    )
                }
                let rid_pid_values: Vec<u8> = (0..MAX_REPETITIONS)
                    .flat_map(|i| {
                        let mut bytes = [0; RID_PID_VALUE_SIZE as usize];
                        bytes[..RID_SIZE as usize]
                            .copy_from_slice(ReservationId::from(i as u64).as_ref());
                        bytes
                    })
                    .collect();
                let rid_pid_value_offset = COMMON_OFFSET;
                let payload_offset = rid_pid_value_offset + rid_pid_values.len() as u32;
                let payload_len = payload_len_kb
                    .map(kb_to_bytes)
                    .unwrap_or(COMMON_PAYLOAD_LEN);
                let res_offset = payload_offset + payload_len;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::ReservationSend]),
                    ),
                    data_segments: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            DataSegment {
                                offset: rid_pid_value_offset,
                                value: rid_pid_values,
                            },
                        ]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[
                                Counter(rid_pid_value_offset, RID_PID_VALUE_SIZE),
                                InstrI32Const(payload_offset),
                                InstrI32Const(payload_len),
                                InstrI32Const(10),
                            ],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_reservation_slots(module, repetitions)
            }
            pub fn gr_reservation_send_commit(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                if !(repetitions <= MAX_REPETITIONS) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= MAX_REPETITIONS",
                    )
                }
                let rid_pid_values: Vec<u8> = (0..MAX_REPETITIONS)
                    .flat_map(|i| {
                        let mut bytes = [0; RID_PID_VALUE_SIZE as usize];
                        bytes[..RID_SIZE as usize]
                            .copy_from_slice(ReservationId::from(i as u64).as_ref());
                        bytes
                    })
                    .collect();
                let rid_pid_value_offset = COMMON_OFFSET;
                let err_handle_offset = rid_pid_value_offset
                    + rid_pid_values.len() as u32;
                let res_offset = err_handle_offset + MAX_REPETITIONS * ERR_HANDLE_SIZE;
                let mut instructions = body::fallible_syscall_instr(
                    MAX_REPETITIONS,
                    1,
                    Counter(err_handle_offset, ERR_HANDLE_SIZE),
                    &[],
                );
                instructions
                    .extend(
                        body::fallible_syscall_instr(
                            repetitions,
                            0,
                            InstrI32Const(res_offset),
                            &[
                                Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
                                InstrI32Load(2, 0),
                                Counter(rid_pid_value_offset, RID_PID_VALUE_SIZE),
                                InstrI32Const(10),
                            ],
                        ),
                    );
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE + 2)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            SysCallName::ReservationSendCommit,
                            SysCallName::SendInit,
                        ]),
                    ),
                    data_segments: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            DataSegment {
                                offset: rid_pid_value_offset,
                                value: rid_pid_values,
                            },
                        ]),
                    ),
                    handle_body: Some(body::from_instructions(instructions)),
                    ..Default::default()
                };
                Self::prepare_handle_with_reservation_slots(module, repetitions)
            }
            pub fn gr_reply(
                r: u32,
                payload_len_kb: Option<u32>,
                wgas: bool,
            ) -> Result<Exec<T>, &'static str> {
                let repetitions = r;
                if !(repetitions <= 1) {
                    ::core::panicking::panic("assertion failed: repetitions <= 1")
                }
                let payload_offset = COMMON_OFFSET;
                let payload_len = payload_len_kb
                    .map(kb_to_bytes)
                    .unwrap_or(COMMON_PAYLOAD_LEN);
                let value_offset = payload_offset + payload_len;
                let res_offset = value_offset + VALUE_SIZE;
                let mut params = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        InstrI32Const(payload_offset),
                        InstrI32Const(payload_len),
                        InstrI32Const(value_offset),
                        InstrI32Const(10),
                    ]),
                );
                let name = match wgas {
                    true => {
                        params.insert(2, InstrI64Const(100_000_000));
                        SysCallName::ReplyWGas
                    }
                    false => SysCallName::Reply,
                };
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([name]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(repetitions, res_offset, &params),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 10000000)
            }
            pub fn gr_reply_commit(r: u32, wgas: bool) -> Result<Exec<T>, &'static str> {
                let repetitions = r;
                if !(repetitions <= 1) {
                    ::core::panicking::panic("assertion failed: repetitions <= 1")
                }
                let value_offset = COMMON_OFFSET;
                let res_offset = value_offset + VALUE_SIZE;
                let mut params = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        InstrI32Const(value_offset),
                        InstrI32Const(10),
                    ]),
                );
                let name = match wgas {
                    true => {
                        params.insert(0, InstrI64Const(100_000_000));
                        SysCallName::ReplyCommitWGas
                    }
                    false => SysCallName::ReplyCommit,
                };
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([name]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(repetitions, res_offset, &params),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 10000000)
            }
            pub fn gr_reply_push(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let payload_offset = COMMON_OFFSET;
                let payload_len = COMMON_PAYLOAD_LEN;
                let res_offset = payload_offset + payload_len;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::ReplyPush]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[InstrI32Const(payload_offset), InstrI32Const(payload_len)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 10_000_000)
            }
            pub fn gr_reply_push_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = 1;
                let payload_offset = COMMON_OFFSET;
                let payload_len = n * 1024;
                let res_offset = payload_offset + payload_len;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::ReplyPush]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[InstrI32Const(payload_offset), InstrI32Const(payload_len)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 10000000)
            }
            pub fn gr_reservation_reply(
                batches: u32,
                payload_len_kb: Option<u32>,
            ) -> Result<Exec<T>, &'static str> {
                let repetitions = batches;
                let max_repetitions = 1;
                if !(repetitions <= max_repetitions) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= max_repetitions",
                    )
                }
                let rid_values: Vec<_> = (0..max_repetitions)
                    .flat_map(|i| {
                        let mut bytes = [0; RID_VALUE_SIZE as usize];
                        bytes[..RID_SIZE as usize]
                            .copy_from_slice(ReservationId::from(i as u64).as_ref());
                        bytes.to_vec()
                    })
                    .collect();
                let rid_value_offset = COMMON_OFFSET;
                let payload_offset = rid_value_offset + rid_values.len() as u32;
                let payload_len = payload_len_kb
                    .map(kb_to_bytes)
                    .unwrap_or(COMMON_PAYLOAD_LEN);
                let res_offset = payload_offset + payload_len;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::ReservationReply]),
                    ),
                    data_segments: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            DataSegment {
                                offset: rid_value_offset,
                                value: rid_values,
                            },
                        ]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[
                                Counter(rid_value_offset, RID_VALUE_SIZE),
                                InstrI32Const(payload_offset),
                                InstrI32Const(payload_len),
                                InstrI32Const(10),
                            ],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_reservation_slots(module, repetitions)
            }
            pub fn gr_reservation_reply_commit(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r;
                let max_repetitions = 1;
                if !(repetitions <= max_repetitions) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= max_repetitions",
                    )
                }
                let rid_values: Vec<_> = (0..max_repetitions)
                    .flat_map(|i| {
                        let mut bytes = [0; RID_VALUE_SIZE as usize];
                        bytes[..RID_SIZE as usize]
                            .copy_from_slice(ReservationId::from(i as u64).as_ref());
                        bytes.to_vec()
                    })
                    .collect();
                let rid_value_offset = COMMON_OFFSET;
                let res_offset = rid_value_offset + rid_values.len() as u32;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::ReservationReplyCommit]),
                    ),
                    data_segments: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            DataSegment {
                                offset: rid_value_offset,
                                value: rid_values,
                            },
                        ]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[
                                Counter(rid_value_offset, RID_VALUE_SIZE),
                                InstrI32Const(10),
                            ],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_reservation_slots(module, repetitions)
            }
            pub fn gr_reservation_reply_commit_per_kb(
                n: u32,
            ) -> Result<Exec<T>, &'static str> {
                let repetitions = 1;
                let rid_value_offset = COMMON_OFFSET;
                let payload_offset = rid_value_offset + RID_VALUE_SIZE;
                let payload_len = n * 1024;
                let res_offset = payload_offset + payload_len;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::ReservationReply]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[
                                InstrI32Const(rid_value_offset),
                                InstrI32Const(payload_offset),
                                InstrI32Const(payload_len),
                                InstrI32Const(10),
                            ],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_reservation_slots(module, repetitions)
            }
            pub fn gr_reply_to(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let res_offset = COMMON_OFFSET;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::ReplyTo]),
                    ),
                    reply_body: Some(
                        body::fallible_syscall(repetitions, res_offset, &[]),
                    ),
                    ..Default::default()
                };
                let instance = Program::<
                    T,
                >::new(module.into(), ::alloc::vec::Vec::new())?;
                let msg_id = MessageId::from(10);
                let msg = Message::new(
                        msg_id,
                        instance.addr.as_bytes().into(),
                        ProgramId::from(
                            instance.caller.clone().into_origin().as_bytes(),
                        ),
                        Default::default(),
                        Some(1_000_000),
                        0,
                        None,
                    )
                    .into_stored();
                MailboxOf::<T>::insert(msg, u32::MAX.unique_saturated_into())
                    .expect("Error during mailbox insertion");
                utils::prepare_exec::<
                    T,
                >(
                    instance.caller.into_origin(),
                    HandleKind::Reply(msg_id, 0),
                    ::alloc::vec::Vec::new(),
                    Default::default(),
                )
            }
            pub fn gr_signal_from(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let res_offset = COMMON_OFFSET;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::SignalFrom]),
                    ),
                    handle_body: Some(
                        body::syscall(repetitions, &[InstrI32Const(res_offset)]),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_reply_input(
                repetitions: u32,
                input_len_kb: Option<u32>,
                wgas: bool,
            ) -> Result<Exec<T>, &'static str> {
                let input_at = 0;
                let input_len = input_len_kb
                    .map(kb_to_bytes)
                    .unwrap_or(COMMON_PAYLOAD_LEN);
                let value_offset = COMMON_OFFSET;
                let res_offset = value_offset + VALUE_SIZE;
                if !(repetitions <= 1) {
                    ::core::panicking::panic("assertion failed: repetitions <= 1")
                }
                if !(input_len <= MAX_PAYLOAD_LEN) {
                    ::core::panicking::panic(
                        "assertion failed: input_len <= MAX_PAYLOAD_LEN",
                    )
                }
                let mut params = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        InstrI32Const(input_at),
                        InstrI32Const(input_len),
                        InstrI32Const(value_offset),
                        InstrI32Const(10),
                    ]),
                );
                let name = match wgas {
                    true => {
                        params.insert(2, InstrI64Const(100_000_000));
                        SysCallName::ReplyInputWGas
                    }
                    false => SysCallName::ReplyInput,
                };
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([name]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(repetitions, res_offset, &params),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_const_payload(module)
            }
            pub fn gr_reply_push_input(
                batches: Option<u32>,
                input_len_kb: Option<u32>,
            ) -> Result<Exec<T>, &'static str> {
                if !(batches.is_some() != input_len_kb.is_some()) {
                    ::core::panicking::panic(
                        "assertion failed: batches.is_some() != input_len_kb.is_some()",
                    )
                }
                let repetitions = batches
                    .map(|batches| batches * API_BENCHMARK_BATCH_SIZE)
                    .unwrap_or(1);
                let input_at = 0;
                let input_len = input_len_kb
                    .map(kb_to_bytes)
                    .unwrap_or(COMMON_PAYLOAD_LEN);
                let res_offset = COMMON_OFFSET;
                if !(input_len <= MAX_PAYLOAD_LEN) {
                    ::core::panicking::panic(
                        "assertion failed: input_len <= MAX_PAYLOAD_LEN",
                    )
                }
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::ReplyPushInput]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[InstrI32Const(input_at), InstrI32Const(input_len)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_const_payload(module)
            }
            pub fn gr_send_input(
                batches: u32,
                input_len_kb: Option<u32>,
                wgas: bool,
            ) -> Result<Exec<T>, &'static str> {
                let repetitions = batches * API_BENCHMARK_BATCH_SIZE;
                let input_at = 0;
                let input_len = input_len_kb
                    .map(kb_to_bytes)
                    .unwrap_or(COMMON_PAYLOAD_LEN);
                let pid_value_offset = COMMON_OFFSET;
                let res_offset = pid_value_offset + PID_VALUE_SIZE;
                if !(repetitions <= MAX_REPETITIONS) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= MAX_REPETITIONS",
                    )
                }
                if !(input_len <= MAX_PAYLOAD_LEN) {
                    ::core::panicking::panic(
                        "assertion failed: input_len <= MAX_PAYLOAD_LEN",
                    )
                }
                let mut params = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        InstrI32Const(pid_value_offset),
                        InstrI32Const(input_at),
                        InstrI32Const(input_len),
                        InstrI32Const(10),
                    ]),
                );
                let name = match wgas {
                    true => {
                        params.insert(3, InstrI64Const(100_000_000));
                        SysCallName::SendInputWGas
                    }
                    false => SysCallName::SendInput,
                };
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([name]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(repetitions, res_offset, &params),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle_with_const_payload(module)
            }
            pub fn gr_send_push_input(
                r: u32,
                input_len_kb: Option<u32>,
            ) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                if !(repetitions <= MAX_REPETITIONS) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= MAX_REPETITIONS",
                    )
                }
                let input_at = 0;
                let input_len = input_len_kb
                    .map(kb_to_bytes)
                    .unwrap_or(COMMON_PAYLOAD_LEN);
                let res_offset = COMMON_OFFSET;
                let err_handle_offset = COMMON_OFFSET + ERR_LEN_SIZE;
                let mut instructions = body::fallible_syscall_instr(
                    MAX_REPETITIONS,
                    1,
                    Counter(err_handle_offset, ERR_HANDLE_SIZE),
                    &[],
                );
                instructions
                    .extend(
                        body::fallible_syscall_instr(
                                repetitions,
                                0,
                                InstrI32Const(res_offset),
                                &[
                                    Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
                                    InstrI32Load(2, 0),
                                    InstrI32Const(input_at),
                                    InstrI32Const(input_len),
                                ],
                            )
                            .into_iter(),
                    );
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            SysCallName::SendPushInput,
                            SysCallName::SendInit,
                        ]),
                    ),
                    handle_body: Some(body::from_instructions(instructions)),
                    ..Default::default()
                };
                Self::prepare_handle_with_const_payload(module)
            }
            pub fn gr_status_code(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let res_offset = COMMON_OFFSET;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::StatusCode]),
                    ),
                    reply_body: Some(
                        body::fallible_syscall(repetitions, res_offset, &[]),
                    ),
                    ..Default::default()
                };
                let instance = Program::<
                    T,
                >::new(module.into(), ::alloc::vec::Vec::new())?;
                let msg_id = MessageId::from(10);
                let msg = Message::new(
                        msg_id,
                        instance.addr.as_bytes().into(),
                        ProgramId::from(
                            instance.caller.clone().into_origin().as_bytes(),
                        ),
                        Default::default(),
                        Some(1_000_000),
                        0,
                        None,
                    )
                    .into_stored();
                MailboxOf::<T>::insert(msg, u32::MAX.unique_saturated_into())
                    .expect("Error during mailbox insertion");
                utils::prepare_exec::<
                    T,
                >(
                    instance.caller.into_origin(),
                    HandleKind::Reply(msg_id, 0),
                    ::alloc::vec::Vec::new(),
                    Default::default(),
                )
            }
            pub fn gr_debug(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let string_offset = COMMON_OFFSET;
                let string_len = COMMON_PAYLOAD_LEN;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Debug]),
                    ),
                    handle_body: Some(
                        body::syscall(
                            repetitions,
                            &[InstrI32Const(string_offset), InstrI32Const(string_len)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_debug_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = API_BENCHMARK_BATCH_SIZE;
                let string_offset = COMMON_OFFSET;
                let string_len = n * 1024;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Debug]),
                    ),
                    handle_body: Some(
                        body::syscall(
                            repetitions,
                            &[InstrI32Const(string_offset), InstrI32Const(string_len)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_error(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                let res_offset = COMMON_OFFSET;
                let err_data_buffer_offset = res_offset + ERR_LEN_SIZE;
                let mut handle_body = body::fallible_syscall(
                    repetitions,
                    res_offset,
                    &[InstrI32Const(err_data_buffer_offset)],
                );
                handle_body
                    .code_mut()
                    .elements_mut()
                    .splice(
                        0..0,
                        [
                            Instruction::I32Const(0),
                            Instruction::I32Const(0),
                            Instruction::Call(0),
                        ],
                    );
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Error]),
                    ),
                    handle_body: Some(handle_body),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn termination_bench(
                name: SysCallName,
                param: Option<u32>,
                r: u32,
            ) -> Result<Exec<T>, &'static str> {
                let repetitions = r;
                if !(repetitions <= 1) {
                    ::core::panicking::panic("assertion failed: repetitions <= 1")
                }
                let params = if let Some(c) = param {
                    if !(name.signature().params.len() == 1) {
                        ::core::panicking::panic(
                            "assertion failed: name.signature().params.len() == 1",
                        )
                    }
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([InstrI32Const(c)]),
                    )
                } else {
                    if !name.signature().params.is_empty() {
                        ::core::panicking::panic(
                            "assertion failed: name.signature().params.is_empty()",
                        )
                    }
                    ::alloc::vec::Vec::new()
                };
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([name]),
                    ),
                    handle_body: Some(body::syscall(repetitions, &params)),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_wake(r: u32) -> Result<Exec<T>, &'static str> {
                let repetitions = r * API_BENCHMARK_BATCH_SIZE;
                if !(repetitions <= MAX_REPETITIONS) {
                    ::core::panicking::panic(
                        "assertion failed: repetitions <= MAX_REPETITIONS",
                    )
                }
                let message_ids: Vec<u8> = (0..MAX_REPETITIONS)
                    .flat_map(|i| {
                        <[u8; MID_SIZE as usize]>::from(MessageId::from(i as u64))
                            .to_vec()
                    })
                    .collect();
                let message_id_offset = COMMON_OFFSET;
                let res_offset = message_id_offset + message_ids.len() as u32;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Wake]),
                    ),
                    data_segments: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            DataSegment {
                                offset: message_id_offset,
                                value: message_ids,
                            },
                        ]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            repetitions,
                            res_offset,
                            &[Counter(message_id_offset, MID_SIZE), InstrI32Const(10)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_create_program(
                batches: u32,
                payload_len_kb: Option<u32>,
                salt_len_kb: Option<u32>,
                wgas: bool,
            ) -> Result<Exec<T>, &'static str> {
                let repetitions = batches * API_BENCHMARK_BATCH_SIZE;
                let module = WasmModule::<T>::dummy();
                let _ = Gear::<
                    T,
                >::upload_code_raw(
                    RawOrigin::Signed(benchmarking::account("instantiator", 0, 0))
                        .into(),
                    module.code,
                );
                let mut cid_value = [0; CID_VALUE_SIZE as usize];
                cid_value[0..CID_SIZE as usize].copy_from_slice(module.hash.as_ref());
                cid_value[CID_SIZE as usize..].copy_from_slice(&0u128.to_le_bytes());
                let cid_value_offset = COMMON_OFFSET;
                let payload_offset = cid_value_offset + cid_value.len() as u32;
                let payload_len = payload_len_kb.map(kb_to_bytes).unwrap_or(10);
                let res_offset = payload_offset + payload_len;
                let salt_offset = res_offset;
                let salt_len = salt_len_kb.map(kb_to_bytes).unwrap_or(32);
                let mut params = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        InstrI32Const(cid_value_offset),
                        InstrI32Const(salt_offset),
                        InstrI32Const(salt_len),
                        InstrI32Const(payload_offset),
                        InstrI32Const(payload_len),
                        InstrI32Const(10),
                    ]),
                );
                let name = match wgas {
                    true => {
                        params.insert(5, InstrI64Const(100_000_000));
                        SysCallName::CreateProgramWGas
                    }
                    false => SysCallName::CreateProgram,
                };
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([name]),
                    ),
                    data_segments: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            DataSegment {
                                offset: cid_value_offset,
                                value: cid_value.to_vec(),
                            },
                        ]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(repetitions, res_offset, &params),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn gr_pay_program_rent(r: u32) -> Result<Exec<T>, &'static str> {
                let pid_value_offset = COMMON_OFFSET;
                let res_offset = pid_value_offset + PID_SIZE + VALUE_SIZE;
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::PayProgramRent]),
                    ),
                    handle_body: Some(
                        body::fallible_syscall(
                            r,
                            res_offset,
                            &[InstrI32Const(pid_value_offset)],
                        ),
                    ),
                    ..Default::default()
                };
                Self::prepare_handle(module, 10_000_000)
            }
            pub fn lazy_pages_signal_read(
                wasm_pages: WasmPage,
            ) -> Result<Exec<T>, &'static str> {
                let instrs = body::read_access_all_pages_instrs(
                    wasm_pages,
                    ::alloc::vec::Vec::new(),
                );
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    handle_body: Some(body::from_instructions(instrs)),
                    stack_end: Some(0.into()),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn lazy_pages_signal_write(
                wasm_pages: WasmPage,
            ) -> Result<Exec<T>, &'static str> {
                let instrs = body::write_access_all_pages_instrs(
                    wasm_pages,
                    ::alloc::vec::Vec::new(),
                );
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    handle_body: Some(body::from_instructions(instrs)),
                    stack_end: Some(0.into()),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn lazy_pages_signal_write_after_read(
                wasm_pages: WasmPage,
            ) -> Result<Exec<T>, &'static str> {
                let instrs = body::read_access_all_pages_instrs(
                    max_pages::<T>().into(),
                    ::alloc::vec::Vec::new(),
                );
                let instrs = body::write_access_all_pages_instrs(wasm_pages, instrs);
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    handle_body: Some(body::from_instructions(instrs)),
                    stack_end: Some(0.into()),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn lazy_pages_load_page_storage_data(
                wasm_pages: WasmPage,
            ) -> Result<Exec<T>, &'static str> {
                let exec = Self::lazy_pages_signal_read(wasm_pages)?;
                let program_id = exec.context.program().id();
                for page in wasm_pages
                    .iter_from_zero()
                    .flat_map(|p| p.to_pages_iter::<GearPage>())
                {
                    ProgramStorageOf::<
                        T,
                    >::set_program_page_data(
                        program_id,
                        page,
                        PageBuf::from_inner(PageBufInner::filled_with(1)),
                    );
                }
                Ok(exec)
            }
            pub fn lazy_pages_host_func_read(
                wasm_pages: WasmPage,
            ) -> Result<Exec<T>, &'static str> {
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Debug]),
                    ),
                    handle_body: Some(
                        body::from_instructions(
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::I32Const(0),
                                    Instruction::I32Const(wasm_pages.offset() as i32),
                                    Instruction::Call(0),
                                ]),
                            ),
                        ),
                    ),
                    stack_end: Some(0.into()),
                    ..Default::default()
                };
                Self::prepare_handle(module, 0)
            }
            pub fn lazy_pages_host_func_write(
                wasm_pages: WasmPage,
            ) -> Result<Exec<T>, &'static str> {
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Read]),
                    ),
                    handle_body: Some(
                        body::from_instructions(
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::I32Const(0),
                                    Instruction::I32Const(wasm_pages.offset() as i32),
                                    Instruction::I32Const(0),
                                    Instruction::I32Const(0),
                                    Instruction::Call(0),
                                ]),
                            ),
                        ),
                    ),
                    stack_end: Some(0.into()),
                    ..Default::default()
                };
                Self::prepare_handle_with_const_payload(module)
            }
            pub fn lazy_pages_host_func_write_after_read(
                wasm_pages: WasmPage,
            ) -> Result<Exec<T>, &'static str> {
                let max_pages = WasmPage::from_offset(MAX_PAYLOAD_LEN);
                if !(wasm_pages <= max_pages) {
                    ::core::panicking::panic("assertion failed: wasm_pages <= max_pages")
                }
                let mut instrs = body::read_access_all_pages_instrs(
                    max_pages,
                    ::alloc::vec::Vec::new(),
                );
                instrs
                    .extend_from_slice(
                        &[
                            Instruction::I32Const(0),
                            Instruction::I32Const(wasm_pages.offset() as i32),
                            Instruction::I32Const(0),
                            Instruction::I32Const(0),
                            Instruction::Call(0),
                        ],
                    );
                let module = ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Read]),
                    ),
                    handle_body: Some(body::from_instructions(instrs)),
                    stack_end: Some(0.into()),
                    ..Default::default()
                };
                Self::prepare_handle_with_const_payload(module)
            }
        }
    }
    mod utils {
        //! Utils for benchmarks.
        use super::Exec;
        use crate::{
            manager::{CodeInfo, ExtManager, HandleKind},
            Config, CostsPerBlockOf, CurrencyOf, DbWeightOf, MailboxOf, Pallet as Gear,
            QueueOf, RentCostPerBlockOf,
        };
        use common::{
            scheduler::SchedulingCostsPerBlock, storage::*, CodeStorage, Origin,
        };
        use core_processor::{
            configs::{BlockConfig, BlockInfo},
            ContextChargedForCode, ContextChargedForInstrumentation,
        };
        use frame_support::traits::{Currency, Get};
        use gear_core::{
            code::{Code, CodeAndId},
            ids::{CodeId, MessageId, ProgramId},
            message::{Dispatch, DispatchKind, Message, ReplyDetails, SignalDetails},
        };
        use sp_core::H256;
        use sp_runtime::traits::UniqueSaturatedInto;
        use sp_std::{convert::TryInto, prelude::*};
        const DEFAULT_BLOCK_NUMBER: u32 = 0;
        const DEFAULT_INTERVAL: u32 = 1_000;
        pub fn prepare_block_config<T>() -> BlockConfig
        where
            T: Config,
            T::AccountId: Origin,
        {
            let block_info = BlockInfo {
                height: Gear::<T>::block_number().unique_saturated_into(),
                timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
            };
            let existential_deposit = CurrencyOf::<T>::minimum_balance()
                .unique_saturated_into();
            let mailbox_threshold = <T as Config>::MailboxThreshold::get();
            let waitlist_cost = CostsPerBlockOf::<T>::waitlist();
            let reserve_for = CostsPerBlockOf::<T>::reserve_for()
                .unique_saturated_into();
            let reservation = CostsPerBlockOf::<T>::reservation()
                .unique_saturated_into();
            let schedule = T::Schedule::get();
            BlockConfig {
                block_info,
                max_pages: T::Schedule::get().limits.memory_pages.into(),
                page_costs: T::Schedule::get().memory_weights.into(),
                existential_deposit,
                outgoing_limit: 2048,
                host_fn_weights: Default::default(),
                forbidden_funcs: Default::default(),
                mailbox_threshold,
                waitlist_cost,
                dispatch_hold_cost: CostsPerBlockOf::<T>::dispatch_stash(),
                reserve_for,
                reservation,
                read_cost: DbWeightOf::<T>::get().reads(1).ref_time(),
                write_cost: DbWeightOf::<T>::get().writes(1).ref_time(),
                write_per_byte_cost: schedule.db_write_per_byte.ref_time(),
                read_per_byte_cost: schedule.db_read_per_byte.ref_time(),
                module_instantiation_byte_cost: schedule
                    .module_instantiation_per_byte
                    .ref_time(),
                max_reservations: T::ReservationsLimit::get(),
                code_instrumentation_cost: schedule.code_instrumentation_cost.ref_time(),
                code_instrumentation_byte_cost: schedule
                    .code_instrumentation_byte_cost
                    .ref_time(),
                rent_cost: RentCostPerBlockOf::<T>::get().unique_saturated_into(),
            }
        }
        pub struct PrepareConfig {
            pub value: u128,
            pub gas_allowance: u64,
            pub gas_limit: u64,
        }
        impl Default for PrepareConfig {
            fn default() -> Self {
                PrepareConfig {
                    value: 0,
                    gas_allowance: u64::MAX,
                    gas_limit: u64::MAX / 2,
                }
            }
        }
        pub fn prepare_exec<T>(
            source: H256,
            kind: HandleKind,
            payload: Vec<u8>,
            config: PrepareConfig,
        ) -> Result<Exec<T>, &'static str>
        where
            T: Config,
            T::AccountId: Origin,
        {
            #[cfg(feature = "std")]
            let _ = env_logger::try_init();
            let ext_manager = ExtManager::<T>::default();
            let bn: u64 = Gear::<T>::block_number().unique_saturated_into();
            let root_message_id = MessageId::from(bn);
            let dispatch = match kind {
                HandleKind::Init(ref code) => {
                    let program_id = ProgramId::generate(
                        CodeId::generate(code),
                        b"bench_salt",
                    );
                    let schedule = T::Schedule::get();
                    let code = Code::try_new(
                            code.clone(),
                            schedule.instruction_weights.version,
                            |module| schedule.rules(module),
                            schedule.limits.stack_height,
                        )
                        .map_err(|_| "Code failed to load")?;
                    let code_and_id = CodeAndId::new(code);
                    let code_info = CodeInfo::from_code_and_id(&code_and_id);
                    let _ = Gear::<T>::set_code_with_metadata(code_and_id, source);
                    ExtManager::<T>::default()
                        .set_program(
                            program_id,
                            &code_info,
                            root_message_id,
                            DEFAULT_BLOCK_NUMBER.saturating_add(DEFAULT_INTERVAL).into(),
                        );
                    Dispatch::new(
                        DispatchKind::Init,
                        Message::new(
                            root_message_id,
                            ProgramId::from_origin(source),
                            program_id,
                            payload.try_into()?,
                            Some(u64::MAX),
                            config.value,
                            None,
                        ),
                    )
                }
                HandleKind::InitByHash(code_id) => {
                    let program_id = ProgramId::generate(code_id, b"bench_salt");
                    let code = T::CodeStorage::get_code(code_id)
                        .ok_or("Code not found in storage")?;
                    let code_info = CodeInfo::from_code(&code_id, &code);
                    ExtManager::<T>::default()
                        .set_program(
                            program_id,
                            &code_info,
                            root_message_id,
                            DEFAULT_BLOCK_NUMBER.saturating_add(DEFAULT_INTERVAL).into(),
                        );
                    Dispatch::new(
                        DispatchKind::Init,
                        Message::new(
                            root_message_id,
                            ProgramId::from_origin(source),
                            program_id,
                            payload.try_into()?,
                            Some(u64::MAX),
                            config.value,
                            None,
                        ),
                    )
                }
                HandleKind::Handle(dest) => {
                    Dispatch::new(
                        DispatchKind::Handle,
                        Message::new(
                            root_message_id,
                            ProgramId::from_origin(source),
                            dest,
                            payload.try_into()?,
                            Some(u64::MAX),
                            config.value,
                            None,
                        ),
                    )
                }
                HandleKind::Reply(msg_id, exit_code) => {
                    let (msg, _bn) = MailboxOf::<
                        T,
                    >::remove(<T::AccountId as Origin>::from_origin(source), msg_id)
                        .map_err(|_| {
                            "Internal error: unable to find message in mailbox"
                        })?;
                    Dispatch::new(
                        DispatchKind::Reply,
                        Message::new(
                            root_message_id,
                            ProgramId::from_origin(source),
                            msg.source(),
                            payload.try_into()?,
                            Some(u64::MAX),
                            config.value,
                            Some(ReplyDetails::new(msg.id(), exit_code).into()),
                        ),
                    )
                }
                HandleKind::Signal(msg_id, status_code) => {
                    let (msg, _bn) = MailboxOf::<
                        T,
                    >::remove(<T::AccountId as Origin>::from_origin(source), msg_id)
                        .map_err(|_| {
                            "Internal error: unable to find message in mailbox"
                        })?;
                    Dispatch::new(
                        DispatchKind::Signal,
                        Message::new(
                            root_message_id,
                            ProgramId::from_origin(source),
                            msg.source(),
                            payload.try_into()?,
                            Some(u64::MAX),
                            config.value,
                            Some(SignalDetails::new(msg.id(), status_code).into()),
                        ),
                    )
                }
            };
            let dispatch = dispatch.into_stored();
            QueueOf::<T>::clear();
            QueueOf::<T>::queue(dispatch).map_err(|_| "Messages storage corrupted")?;
            let queued_dispatch = match QueueOf::<T>::dequeue()
                .map_err(|_| "MQ storage corrupted")?
            {
                Some(d) => d,
                None => return Err("Dispatch not found"),
            };
            let actor_id = queued_dispatch.destination();
            let actor = ext_manager
                .get_actor(actor_id)
                .ok_or("Program not found in the storage")?;
            let block_config = prepare_block_config::<T>();
            let precharged_dispatch = core_processor::precharge_for_program(
                    &block_config,
                    config.gas_allowance,
                    queued_dispatch.into_incoming(config.gas_limit),
                    actor_id,
                )
                .map_err(|_| "core_processor::precharge_for_program failed")?;
            let balance = actor.balance;
            let context = core_processor::precharge_for_code_length(
                    &block_config,
                    precharged_dispatch,
                    actor_id,
                    actor.executable_data,
                )
                .map_err(|_| "core_processor::precharge_for_code failed")?;
            let code = T::CodeStorage::get_code(context.actor_data().code_id)
                .ok_or("Program code not found")?;
            let context = ContextChargedForCode::from((
                context,
                code.code().len() as u32,
            ));
            let context = core_processor::precharge_for_memory(
                    &block_config,
                    ContextChargedForInstrumentation::from(context),
                )
                .map_err(|_| "core_processor::precharge_for_memory failed")?;
            let origin = ProgramId::from_origin(source);
            Ok(Exec {
                ext_manager,
                block_config,
                context: (context, code, balance, origin).into(),
                random_data: (::alloc::vec::from_elem(0u8, 32), 0),
                memory_pages: Default::default(),
            })
        }
    }
    use syscalls::Benches;
    mod tests {
        //! This module contains pallet tests usually defined under "std" feature in the separate `tests` module.
        //! The reason of moving them here is an ability to run these tests with different execution environments
        //! (native or wasm, i.e. using wasmi or sandbox executors). When "std" is enabled we can run them on wasmi,
        //! when it's not (only "runtime-benchmarks") - sandbox will be turned on.
        use super::*;
        pub mod syscalls_integrity {
            //! Testing integration level of sys-calls
            //!
            //! Integration level is the level between the user (`gcore`/`gstd`) and `core-backend`.
            //! Tests here does not check complex business logic, but only the fact that all the
            //! requested data is received properly, i.e., pointers receive expected types, no export func
            //! signature map errors.
            //!
            //! `gr_read` is tested in the `test_syscall` program by calling `msg::load` to decode each sys-call type.
            //! `gr_exit` and `gr_wait*` call are not intended to be tested with the integration level tests, but only
            //! with business logic tests in the separate module.
            use super::*;
            use crate::{Event, RentCostPerBlockOf, WaitlistOf};
            use frame_support::traits::Randomness;
            use gear_core::ids::{CodeId, ReservationId};
            use gear_core_errors::{ExtError, MessageError};
            use gear_wasm_instrument::syscalls::SysCallName;
            use pallet_timestamp::Pallet as TimestampPallet;
            use parity_scale_codec::Decode;
            use sp_runtime::SaturatedConversion;
            use test_syscalls::{Kind, WASM_BINARY as SYSCALLS_TEST_WASM_BINARY};
            pub fn main_test<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                SysCallName::all()
                    .for_each(|sys_call| {
                        {
                            let lvl = ::log::Level::Info;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api_log(
                                    format_args!("run test for {0:?}", sys_call),
                                    lvl,
                                    &(
                                        "pallet_gear::benchmarking::tests::syscalls_integrity",
                                        "pallet_gear::benchmarking::tests::syscalls_integrity",
                                        "pallets/gear/src/benchmarking/tests/syscalls_integrity.rs",
                                        48u32,
                                    ),
                                    ::log::__private_api::Option::None,
                                );
                            }
                        };
                        match sys_call {
                            SysCallName::Send => check_send::<T>(None),
                            SysCallName::SendWGas => {
                                check_send::<T>(Some(25_000_000_000))
                            }
                            SysCallName::SendCommit => check_send_raw::<T>(None),
                            SysCallName::SendCommitWGas => {
                                check_send_raw::<T>(Some(25_000_000_000))
                            }
                            SysCallName::SendInit | SysCallName::SendPush => {}
                            SysCallName::SendInput => check_send_input::<T>(None),
                            SysCallName::SendPushInput => check_send_push_input::<T>(),
                            SysCallName::SendInputWGas => {
                                check_send_input::<T>(Some(25_000_000_000))
                            }
                            SysCallName::Reply => check_reply::<T>(None),
                            SysCallName::ReplyWGas => {
                                check_reply::<T>(Some(25_000_000_000))
                            }
                            SysCallName::ReplyCommit => check_reply_raw::<T>(None),
                            SysCallName::ReplyCommitWGas => {
                                check_reply_raw::<T>(Some(25_000_000_000))
                            }
                            SysCallName::ReplyTo => check_reply_details::<T>(),
                            SysCallName::SignalFrom => check_signal_details::<T>(),
                            SysCallName::ReplyPush => {}
                            SysCallName::ReplyInput => check_reply_input::<T>(None),
                            SysCallName::ReplyPushInput => check_reply_push_input::<T>(),
                            SysCallName::ReplyInputWGas => {
                                check_reply_input::<T>(Some(25_000_000_000))
                            }
                            SysCallName::CreateProgram => check_create_program::<T>(None),
                            SysCallName::CreateProgramWGas => {
                                check_create_program::<T>(Some(25_000_000_000))
                            }
                            SysCallName::ReplyDeposit => check_gr_reply_deposit::<T>(),
                            SysCallName::Read => {}
                            SysCallName::Size => check_gr_size::<T>(),
                            SysCallName::StatusCode => {}
                            SysCallName::MessageId => check_gr_message_id::<T>(),
                            SysCallName::ProgramId => check_gr_program_id::<T>(),
                            SysCallName::Source => check_gr_source::<T>(),
                            SysCallName::Value => check_gr_value::<T>(),
                            SysCallName::BlockHeight => check_gr_block_height::<T>(),
                            SysCallName::BlockTimestamp => {
                                check_gr_block_timestamp::<T>()
                            }
                            SysCallName::Origin => check_gr_origin::<T>(),
                            SysCallName::GasAvailable => check_gr_gas_available::<T>(),
                            SysCallName::ValueAvailable => {
                                check_gr_value_available::<T>()
                            }
                            SysCallName::Exit
                            | SysCallName::Leave
                            | SysCallName::Wait
                            | SysCallName::WaitFor
                            | SysCallName::WaitUpTo
                            | SysCallName::Wake
                            | SysCallName::Debug
                            | SysCallName::Panic
                            | SysCallName::OomPanic => {}
                            SysCallName::Alloc => check_mem::<T>(false),
                            SysCallName::Free => check_mem::<T>(true),
                            SysCallName::OutOfGas | SysCallName::OutOfAllowance => {}
                            SysCallName::Error => check_gr_err::<T>(),
                            SysCallName::Random => check_gr_random::<T>(),
                            SysCallName::ReserveGas => check_gr_reserve_gas::<T>(),
                            SysCallName::UnreserveGas => check_gr_unreserve_gas::<T>(),
                            SysCallName::ReservationSend => {
                                check_gr_reservation_send::<T>()
                            }
                            SysCallName::ReservationSendCommit => {
                                check_gr_reservation_send_commit::<T>()
                            }
                            SysCallName::ReservationReply => {
                                check_gr_reservation_reply::<T>()
                            }
                            SysCallName::ReservationReplyCommit => {
                                check_gr_reservation_reply_commit::<T>()
                            }
                            SysCallName::SystemReserveGas => {
                                check_gr_system_reserve_gas::<T>()
                            }
                            SysCallName::PayProgramRent => {
                                check_gr_pay_program_rent::<T>()
                            }
                        }
                    });
            }
            fn check_gr_pay_program_rent<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|tester_pid, _| {
                    let default_account = utils::default_account();
                    <T as pallet::Config>::Currency::deposit_creating(
                        &default_account,
                        100_000_000_000_000_u128.unique_saturated_into(),
                    );
                    let block_count = 10;
                    let unused_rent: BalanceOf<T> = 1u32.into();
                    let rent = RentCostPerBlockOf::<T>::get() * block_count.into()
                        + unused_rent;
                    let mp = MessageParamsBuilder::new(
                            <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([
                                        Kind::PayProgramRent(
                                            tester_pid.into_origin().into(),
                                            rent.saturated_into(),
                                            Some((unused_rent.saturated_into(), block_count)),
                                        ),
                                    ]),
                                )
                                .encode(),
                        )
                        .with_value(10_000_000_000);
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                });
            }
            fn check_gr_system_reserve_gas<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|tester_pid, _| {
                    let reserve_amount = 10_000_000;
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let post_check = move || {
                        if !WaitlistOf::<T>::contains(&tester_pid, &next_user_mid) {
                            ::core::panicking::panic_fmt(
                                format_args!("wait list post check failed"),
                            )
                        }
                        match (
                            &Ok(reserve_amount),
                            &GasHandlerOf::<T>::get_system_reserve(next_user_mid),
                        ) {
                            (left_val, right_val) => {
                                if !(*left_val == *right_val) {
                                    let kind = ::core::panicking::AssertKind::Eq;
                                    ::core::panicking::assert_failed(
                                        kind,
                                        &*left_val,
                                        &*right_val,
                                        ::core::option::Option::Some(
                                            format_args!("system reserve gas post check failed"),
                                        ),
                                    );
                                }
                            }
                        };
                    };
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::SystemReserveGas(reserve_amount),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), Some(post_check))
                });
            }
            fn check_gr_reply_deposit<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let deposit_amount = 10_000_000;
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let outgoing_mid = MessageId::generate_outgoing(next_user_mid, 0);
                    let future_reply_id = MessageId::generate_reply(outgoing_mid);
                    let post_check = move || {
                        if !GasHandlerOf::<T>::exists_and_deposit(future_reply_id) {
                            ::core::panicking::panic_fmt(
                                format_args!("gas tree post check failed"),
                            )
                        }
                        match (
                            &Ok(deposit_amount),
                            &GasHandlerOf::<T>::get_limit(future_reply_id),
                        ) {
                            (left_val, right_val) => {
                                if !(*left_val == *right_val) {
                                    let kind = ::core::panicking::AssertKind::Eq;
                                    ::core::panicking::assert_failed(
                                        kind,
                                        &*left_val,
                                        &*right_val,
                                        ::core::option::Option::Some(
                                            format_args!("reply deposit gas post check failed"),
                                        ),
                                    );
                                }
                            }
                        };
                    };
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::ReplyDeposit(deposit_amount),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), Some(post_check))
                });
            }
            fn check_gr_reservation_send<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::ReservationSend(expected_mid.into()),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                });
            }
            fn check_gr_reservation_send_commit<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let payload = b"HI_RSC!!";
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_mid = MessageId::generate_outgoing(next_user_mid, 1);
                    let post_test = move || {
                        if !MailboxOf::<T>::iter_key(default_sender)
                            .any(|(m, _)| {
                                m.id() == expected_mid && m.payload() == payload.to_vec()
                            })
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("No message with expected id found in queue"),
                            )
                        }
                    };
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::ReservationSendRaw(
                                    payload.to_vec(),
                                    expected_mid.into(),
                                ),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), Some(post_test))
                });
            }
            fn check_gr_reservation_reply<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let expected_mid = MessageId::generate_reply(next_user_mid);
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::ReservationReply(expected_mid.into()),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_reservation_reply_commit<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let payload = b"HI_RRC!!";
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_mid = MessageId::generate_reply(next_user_mid);
                    let post_test = move || {
                        let source = ProgramId::from_origin(
                            default_sender.into_origin(),
                        );
                        if !SystemPallet::<T>::events()
                            .into_iter()
                            .any(|e| {
                                let bytes = e.event.encode();
                                let Ok(gear_event): Result<Event<T>, _> = Event::decode(
                                    &mut bytes[1..].as_ref(),
                                ) else { return false };
                                match gear_event {
                                    Event::UserMessageSent {
                                        message,
                                        ..
                                    } if message.id() == expected_mid
                                        && message.payload() == payload
                                        && message.destination() == source => true,
                                    _ => false,
                                }
                            })
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("No message with expected id found in events"),
                            )
                        }
                    };
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::ReservationReplyCommit(
                                    payload.to_vec(),
                                    expected_mid.into(),
                                ),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), Some(post_test))
                });
            }
            fn check_mem<T>(check_free: bool)
            where
                T: Config,
                T::AccountId: Origin,
            {
                #[cfg(feature = "std")] utils::init_logger();
                let wasm_module = alloc_free_test_wasm::<T>();
                let default_account = utils::default_account();
                <T as pallet::Config>::Currency::deposit_creating(
                    &default_account,
                    100_000_000_000_000_u128.unique_saturated_into(),
                );
                Gear::<
                    T,
                >::upload_program(
                        RawOrigin::Signed(default_account.clone()).into(),
                        wasm_module.code,
                        b"alloc-free-test".to_vec(),
                        b"".to_vec(),
                        50_000_000_000,
                        0u128.unique_saturated_into(),
                    )
                    .expect("failed to upload test program");
                let pid = ProgramId::generate(wasm_module.hash, b"alloc-free-test");
                utils::run_to_next_block::<T>(None);
                if !MailboxOf::<T>::is_empty(&default_account) {
                    ::core::panicking::panic(
                        "assertion failed: MailboxOf::<T>::is_empty(&default_account)",
                    )
                }
                if check_free {
                    Gear::<
                        T,
                    >::send_message(
                            RawOrigin::Signed(default_account.clone()).into(),
                            pid,
                            b"".to_vec(),
                            50_000_000_000,
                            0u128.unique_saturated_into(),
                        )
                        .expect("failed to send message to test program");
                    utils::run_to_next_block::<T>(None);
                    if !MailboxOf::<T>::is_empty(&default_account) {
                        ::core::panicking::panic(
                            "assertion failed: MailboxOf::<T>::is_empty(&default_account)",
                        )
                    }
                }
                Gear::<T>::reset();
            }
            fn check_gr_err<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let message_value = u128::MAX;
                    let expected_err = ExtError::Message(MessageError::NotEnoughValue {
                        message_value,
                        value_left: 0,
                    });
                    let expected_err = {
                        let res = ::alloc::fmt::format(
                            format_args!("API error: {0}", expected_err),
                        );
                        res
                    };
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::Error(message_value, expected_err),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                });
            }
            fn check_gr_size<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let expected_size = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([Kind::Size(0)]),
                        )
                        .encoded_size() as u32;
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([Kind::Size(expected_size)]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                });
            }
            fn check_gr_message_id<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::MessageId(next_user_mid.into()),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_program_id<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|id, _| {
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([Kind::ProgramId(id.into())]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_source<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let message_sender = benchmarking::account::<
                        T::AccountId,
                    >("some_user", 0, 0);
                    <T as pallet::Config>::Currency::deposit_creating(
                        &message_sender,
                        50_000_000_000_000_u128.unique_saturated_into(),
                    );
                    let mp = MessageParamsBuilder::new(
                            <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([
                                        Kind::Source(
                                            message_sender.clone().into_origin().to_fixed_bytes(),
                                        ),
                                    ]),
                                )
                                .encode(),
                        )
                        .with_sender(message_sender);
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_value<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let sending_value = u16::MAX as u128;
                    let mp = MessageParamsBuilder::new(
                            <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([Kind::Value(sending_value)]),
                                )
                                .encode(),
                        )
                        .with_value(sending_value);
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_value_available<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let sending_value = 10_000;
                    let mp = MessageParamsBuilder::new(
                            <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([
                                        Kind::ValueAvailable(sending_value - 2000),
                                    ]),
                                )
                                .encode(),
                        )
                        .with_value(sending_value);
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_create_program<T>(gas: Option<u64>)
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);
                    let salt = 10u64;
                    let expected_pid = ProgramId::generate(
                        simplest_gear_wasm::<T>().hash,
                        &salt.to_le_bytes(),
                    );
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::CreateProgram(
                                    salt,
                                    gas,
                                    (expected_mid.into(), expected_pid.into()),
                                ),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                });
            }
            fn check_send<T>(gas: Option<u64>)
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);
                    let payload = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::Send(gas, expected_mid.into()),
                            ]),
                        )
                        .encode();
                    {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api_log(
                                format_args!("payload = {0:?}", payload),
                                lvl,
                                &(
                                    "pallet_gear::benchmarking::tests::syscalls_integrity",
                                    "pallet_gear::benchmarking::tests::syscalls_integrity",
                                    "pallets/gear/src/benchmarking/tests/syscalls_integrity.rs",
                                    498u32,
                                ),
                                ::log::__private_api::Option::None,
                            );
                        }
                    };
                    let mp = payload.into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                });
            }
            /// Tests send_init, send_push, send_commit or send_commit_wgas depending on `gas` param.
            fn check_send_raw<T>(gas: Option<u64>)
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let payload = b"HI_SR!!";
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_mid = MessageId::generate_outgoing(next_user_mid, 2);
                    let post_test = move || {
                        if !MailboxOf::<T>::iter_key(default_sender)
                            .any(|(m, _)| {
                                m.id() == expected_mid && m.payload() == payload.to_vec()
                            })
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("No message with expected id found in queue"),
                            )
                        }
                    };
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::SendRaw(payload.to_vec(), gas, expected_mid.into()),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), Some(post_test))
                });
            }
            fn check_send_input<T>(gas: Option<u64>)
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_message_id = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_message_id = MessageId::generate_outgoing(
                        next_message_id,
                        0,
                    );
                    let payload = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::SendInput(gas, expected_message_id.into()),
                            ]),
                        )
                        .encode();
                    let message = payload.clone().into();
                    let post_test = move || {
                        if !MailboxOf::<T>::iter_key(default_sender)
                            .any(|(m, _)| {
                                m.id() == expected_message_id && m.payload() == payload
                            })
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("No message with expected id found in queue"),
                            )
                        }
                    };
                    (TestCall::send_message(message), Some(post_test))
                });
            }
            #[track_caller]
            fn check_send_push_input<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_message_id = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_message_id = MessageId::generate_outgoing(
                        next_message_id,
                        2,
                    );
                    let payload = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::SendPushInput(expected_message_id.into()),
                            ]),
                        )
                        .encode();
                    let message = payload.clone().into();
                    let post_test = move || {
                        if !MailboxOf::<T>::iter_key(default_sender)
                            .any(|(m, _)| {
                                m.id() == expected_message_id && m.payload() == payload
                            })
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("No message with expected id found in queue"),
                            )
                        }
                    };
                    (TestCall::send_message(message), Some(post_test))
                });
            }
            fn check_reply<T>(gas: Option<u64>)
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let expected_mid = MessageId::generate_reply(next_user_mid);
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::Reply(gas, expected_mid.into()),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_reply_raw<T>(gas: Option<u64>)
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let payload = b"HI_RR!!";
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_mid = MessageId::generate_reply(next_user_mid);
                    let post_test = move || {
                        let source = ProgramId::from_origin(
                            default_sender.into_origin(),
                        );
                        if !SystemPallet::<T>::events()
                            .into_iter()
                            .any(|e| {
                                let bytes = e.event.encode();
                                let Ok(gear_event): Result<Event<T>, _> = Event::decode(
                                    &mut bytes[1..].as_ref(),
                                ) else { return false };
                                match gear_event {
                                    Event::UserMessageSent {
                                        message,
                                        ..
                                    } if message.id() == expected_mid
                                        && message.payload() == payload
                                        && message.destination() == source => true,
                                    _ => false,
                                }
                            })
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("No message with expected id found in events"),
                            )
                        }
                    };
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::ReplyRaw(payload.to_vec(), gas, expected_mid.into()),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), Some(post_test))
                });
            }
            fn check_reply_input<T>(gas: Option<u64>)
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_message_id = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_message_id = MessageId::generate_reply(next_message_id);
                    let payload = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::ReplyInput(gas, expected_message_id.into()),
                            ]),
                        )
                        .encode();
                    let message = payload.clone().into();
                    let post_test = move || {
                        let source = ProgramId::from_origin(
                            default_sender.into_origin(),
                        );
                        if !SystemPallet::<T>::events()
                            .into_iter()
                            .any(|e| {
                                let bytes = e.event.encode();
                                let Ok(gear_event): Result<Event<T>, _> = Event::decode(
                                    &mut bytes[1..].as_ref(),
                                ) else { return false };
                                match gear_event {
                                    Event::UserMessageSent {
                                        message,
                                        ..
                                    } if message.id() == expected_message_id
                                        && message.payload() == payload
                                        && message.destination() == source => true,
                                    _ => false,
                                }
                            })
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("No message with expected id found in events"),
                            )
                        }
                    };
                    (TestCall::send_message(message), Some(post_test))
                });
            }
            fn check_reply_push_input<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_message_id = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_message_id = MessageId::generate_reply(next_message_id);
                    let payload = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::ReplyPushInput(expected_message_id.into()),
                            ]),
                        )
                        .encode();
                    let message = payload.clone().into();
                    let post_test = move || {
                        let source = ProgramId::from_origin(
                            default_sender.into_origin(),
                        );
                        if !SystemPallet::<T>::events()
                            .into_iter()
                            .any(|e| {
                                let bytes = e.event.encode();
                                let Ok(gear_event): Result<Event<T>, _> = Event::decode(
                                    &mut bytes[1..].as_ref(),
                                ) else { return false };
                                match gear_event {
                                    Event::UserMessageSent {
                                        message,
                                        ..
                                    } if message.id() == expected_message_id
                                        && message.payload() == payload
                                        && message.destination() == source => true,
                                    _ => false,
                                }
                            })
                        {
                            ::core::panicking::panic_fmt(
                                format_args!("No message with expected id found in events"),
                            )
                        }
                    };
                    (TestCall::send_message(message), Some(post_test))
                });
            }
            fn check_reply_details<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|tester_pid, _| {
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);
                    Gear::<
                        T,
                    >::send_message(
                            RawOrigin::Signed(default_sender.clone()).into(),
                            tester_pid,
                            <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([
                                        Kind::ReplyDetails([255u8; 32], 0),
                                    ]),
                                )
                                .encode(),
                            50_000_000_000,
                            0u128.unique_saturated_into(),
                        )
                        .expect("triggering message send to mailbox failed");
                    utils::run_to_next_block::<T>(None);
                    let reply_to = MailboxOf::<T>::iter_key(default_sender)
                        .last()
                        .map(|(m, _)| m)
                        .expect("no mail found after invoking sys-call test program");
                    match (&reply_to.id(), &expected_mid) {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(
                                    kind,
                                    &*left_val,
                                    &*right_val,
                                    ::core::option::Option::Some(
                                        format_args!("mailbox check failed"),
                                    ),
                                );
                            }
                        }
                    };
                    let mp = MessageParamsBuilder::new(
                            Kind::ReplyDetails(expected_mid.into(), 0).encode(),
                        )
                        .with_reply_id(reply_to.id());
                    (TestCall::send_reply(mp), None::<DefaultPostCheck>)
                });
            }
            fn check_signal_details<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|tester_pid, _| {
                    let default_sender = utils::default_account::<T::AccountId>();
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(default_sender.clone());
                    let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);
                    Gear::<
                        T,
                    >::send_message(
                            RawOrigin::Signed(default_sender.clone()).into(),
                            tester_pid,
                            <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([Kind::SignalDetails]),
                                )
                                .encode(),
                            50_000_000_000,
                            0u128.unique_saturated_into(),
                        )
                        .expect("triggering message send to mailbox failed");
                    utils::run_to_next_block::<T>(None);
                    let reply_to = MailboxOf::<T>::iter_key(default_sender)
                        .last()
                        .map(|(m, _)| m)
                        .expect("no mail found after invoking sys-call test program");
                    match (&reply_to.id(), &expected_mid) {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(
                                    kind,
                                    &*left_val,
                                    &*right_val,
                                    ::core::option::Option::Some(
                                        format_args!("mailbox check failed"),
                                    ),
                                );
                            }
                        }
                    };
                    let mp = MessageParamsBuilder::new(Kind::SignalDetailsWake.encode())
                        .with_reply_id(reply_to.id());
                    (TestCall::send_reply(mp), None::<DefaultPostCheck>)
                });
            }
            fn check_gr_block_height<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let current_height: u32 = SystemPallet::<T>::block_number()
                        .unique_saturated_into();
                    let height_delta = 15;
                    utils::run_to_block::<T>(current_height + height_delta, None);
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::BlockHeight(current_height + height_delta + 1),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_block_timestamp<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let block_timestamp = 125;
                    TimestampPallet::<
                        T,
                    >::set(
                            RawOrigin::None.into(),
                            block_timestamp.unique_saturated_into(),
                        )
                        .expect("failed to put timestamp");
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::BlockTimestamp(block_timestamp),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_origin<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|tester_id, _| {
                    use demo_proxy::{InputArgs, WASM_BINARY as PROXY_WASM_BINARY};
                    let default_sender = utils::default_account::<T::AccountId>();
                    let message_sender = benchmarking::account::<
                        T::AccountId,
                    >("some_user", 0, 0);
                    <T as pallet::Config>::Currency::deposit_creating(
                        &message_sender,
                        100_000_000_000_000_u128.unique_saturated_into(),
                    );
                    let payload = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::Origin(
                                    message_sender.clone().into_origin().to_fixed_bytes(),
                                ),
                            ]),
                        )
                        .encode();
                    Gear::<
                        T,
                    >::upload_program(
                            RawOrigin::Signed(default_sender).into(),
                            PROXY_WASM_BINARY.to_vec(),
                            b"".to_vec(),
                            InputArgs {
                                destination: tester_id.into_origin().into(),
                            }
                                .encode(),
                            50_000_000_000,
                            0u128.unique_saturated_into(),
                        )
                        .expect("failed deploying proxy");
                    let proxy_pid = ProgramId::generate(
                        CodeId::generate(PROXY_WASM_BINARY),
                        b"",
                    );
                    utils::run_to_next_block::<T>(None);
                    Gear::<
                        T,
                    >::send_message(
                            RawOrigin::Signed(message_sender.clone()).into(),
                            proxy_pid,
                            payload.clone(),
                            50_000_000_000,
                            0u128.unique_saturated_into(),
                        )
                        .expect("failed setting origin");
                    utils::run_to_next_block::<T>(None);
                    let mp = MessageParamsBuilder::new(payload)
                        .with_sender(message_sender);
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_reserve_gas<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let next_user_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let expected_reservation_id = ReservationId::generate(
                            next_user_mid,
                            2,
                        )
                        .encode();
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::Reserve(expected_reservation_id),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_unreserve_gas<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([Kind::Unreserve(10_000)]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_random<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let next_mid = utils::get_next_message_id::<
                        T,
                    >(utils::default_account::<T::AccountId>());
                    let (random, expected_bn) = T::Randomness::random(next_mid.as_ref());
                    #[cfg(feature = "std")]
                    let expected_bn = expected_bn + One::one();
                    let salt = [1; 32];
                    let expected_hash = {
                        let mut salt_vec = salt.to_vec();
                        salt_vec.extend_from_slice(random.as_ref());
                        sp_io::hashing::blake2_256(&salt_vec)
                    };
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                Kind::Random(
                                    salt,
                                    (expected_hash, expected_bn.unique_saturated_into()),
                                ),
                            ]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn check_gr_gas_available<T>()
            where
                T: Config,
                T::AccountId: Origin,
            {
                run_tester::<
                    T,
                    _,
                    _,
                    T::AccountId,
                >(|_, _| {
                    let lower = 50_000_000_000 - 1_000_000_000;
                    let upper = 50_000_000_000 - 200_000_000;
                    let mp = <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([Kind::GasAvailable(lower, upper)]),
                        )
                        .encode()
                        .into();
                    (TestCall::send_message(mp), None::<DefaultPostCheck>)
                })
            }
            fn run_tester<T, P, S, Id>(get_test_call_params: S)
            where
                T: Config + frame_system::Config<AccountId = Id>,
                T::RuntimeOrigin: From<RawOrigin<Id>>,
                Id: Clone + Origin,
                P: FnOnce(),
                S: FnOnce(ProgramId, CodeId) -> (TestCall<Id>, Option<P>),
            {
                #[cfg(feature = "std")] utils::init_logger();
                let child_wasm = simplest_gear_wasm::<T>();
                let child_code = child_wasm.code;
                let child_code_hash = child_wasm.hash;
                let tester_pid = ProgramId::generate(
                    CodeId::generate(SYSCALLS_TEST_WASM_BINARY),
                    b"",
                );
                let child_deployer = benchmarking::account::<
                    T::AccountId,
                >("child_deployer", 0, 0);
                <T as pallet::Config>::Currency::deposit_creating(
                    &child_deployer,
                    100_000_000_000_000_u128.unique_saturated_into(),
                );
                Gear::<
                    T,
                >::upload_program(
                        RawOrigin::Signed(child_deployer).into(),
                        child_code,
                        ::alloc::vec::Vec::new(),
                        ::alloc::vec::Vec::new(),
                        50_000_000_000,
                        0u128.unique_saturated_into(),
                    )
                    .expect("child program deploy failed");
                let default_account = utils::default_account();
                <T as pallet::Config>::Currency::deposit_creating(
                    &default_account,
                    100_000_000_000_000_u128.unique_saturated_into(),
                );
                Gear::<
                    T,
                >::upload_program(
                        RawOrigin::Signed(default_account).into(),
                        SYSCALLS_TEST_WASM_BINARY.to_vec(),
                        b"".to_vec(),
                        child_code_hash.encode(),
                        50_000_000_000,
                        0u128.unique_saturated_into(),
                    )
                    .expect("sys-call check program deploy failed");
                utils::run_to_next_block::<T>(None);
                let (call, post_check) = get_test_call_params(
                    tester_pid,
                    child_code_hash,
                );
                let sender;
                match call {
                    TestCall::SendMessage(mp) => {
                        sender = mp.sender.clone();
                        Gear::<
                            T,
                        >::send_message(
                                RawOrigin::Signed(mp.sender).into(),
                                tester_pid,
                                mp.payload,
                                50_000_000_000,
                                mp.value.unique_saturated_into(),
                            )
                            .expect("failed send message");
                    }
                    TestCall::SendReply(rp) => {
                        sender = rp.sender.clone();
                        Gear::<
                            T,
                        >::send_reply(
                                RawOrigin::Signed(rp.sender).into(),
                                rp.reply_to_id,
                                rp.payload,
                                50_000_000_000,
                                rp.value.unique_saturated_into(),
                            )
                            .expect("failed send reply");
                    }
                }
                utils::run_to_next_block::<T>(None);
                let ok_mails = MailboxOf::<T>::iter_key(sender)
                    .filter(|(m, _)| m.payload() == b"ok")
                    .count();
                match (&ok_mails, &1) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
                if let Some(post_check) = post_check {
                    post_check();
                }
                Gear::<T>::reset();
                <T as pallet::Config>::Currency::slash(
                    &Id::from_origin(tester_pid.into_origin()),
                    <T as pallet::Config>::Currency::free_balance(
                        &Id::from_origin(tester_pid.into_origin()),
                    ),
                );
            }
            type DefaultPostCheck = fn() -> ();
            enum TestCall<Id> {
                SendMessage(SendMessageParams<Id>),
                SendReply(SendReplyParams<Id>),
            }
            impl<Id: Origin> TestCall<Id> {
                fn send_message(mp: MessageParamsBuilder<Id>) -> Self {
                    TestCall::SendMessage(mp.build_send_message())
                }
                fn send_reply(mp: MessageParamsBuilder<Id>) -> Self {
                    TestCall::SendReply(mp.build_send_reply())
                }
            }
            struct SendMessageParams<Id> {
                sender: Id,
                payload: Vec<u8>,
                value: u128,
            }
            struct SendReplyParams<Id> {
                sender: Id,
                reply_to_id: MessageId,
                payload: Vec<u8>,
                value: u128,
            }
            struct MessageParamsBuilder<Id> {
                sender: Id,
                payload: Vec<u8>,
                value: Option<u128>,
                reply_to_id: Option<MessageId>,
            }
            impl<Id: Origin> MessageParamsBuilder<Id> {
                fn with_sender(mut self, sender: Id) -> Self {
                    self.sender = sender;
                    self
                }
                fn with_value(mut self, value: u128) -> Self {
                    self.value = Some(value);
                    self
                }
                fn with_reply_id(mut self, reply_to_id: MessageId) -> Self {
                    self.reply_to_id = Some(reply_to_id);
                    self
                }
                fn build_send_message(self) -> SendMessageParams<Id> {
                    let MessageParamsBuilder { sender, payload, value, .. } = self;
                    SendMessageParams {
                        sender,
                        payload,
                        value: value.unwrap_or(0),
                    }
                }
                fn build_send_reply(self) -> SendReplyParams<Id> {
                    let MessageParamsBuilder { sender, payload, value, reply_to_id } = self;
                    SendReplyParams {
                        sender,
                        reply_to_id: reply_to_id
                            .expect("internal error: reply id wasn't set"),
                        payload,
                        value: value.unwrap_or(0),
                    }
                }
            }
            impl<Id: Origin> MessageParamsBuilder<Id> {
                fn new(payload: Vec<u8>) -> Self {
                    let sender = utils::default_account();
                    Self {
                        payload,
                        sender,
                        value: None,
                        reply_to_id: None,
                    }
                }
            }
            impl<Id: Origin> From<Vec<u8>> for MessageParamsBuilder<Id> {
                fn from(v: Vec<u8>) -> Self {
                    MessageParamsBuilder::new(v)
                }
            }
            fn simplest_gear_wasm<T: Config>() -> WasmModule<T>
            where
                T::AccountId: Origin,
            {
                ModuleDefinition {
                    memory: Some(ImportedMemory::new(1)),
                    ..Default::default()
                }
                    .into()
            }
            fn alloc_free_test_wasm<T: Config>() -> WasmModule<T>
            where
                T::AccountId: Origin,
            {
                use gear_wasm_instrument::parity_wasm::elements::{
                    FuncBody, Instructions,
                };
                ModuleDefinition {
                    memory: Some(ImportedMemory::new(1)),
                    imported_functions: <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([SysCallName::Alloc, SysCallName::Free]),
                    ),
                    init_body: Some(
                        FuncBody::new(
                            ::alloc::vec::Vec::new(),
                            Instructions::new(
                                <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([
                                        Instruction::Block(BlockType::NoResult),
                                        Instruction::I32Const(0x2),
                                        Instruction::Call(0),
                                        Instruction::I32Const(0x1),
                                        Instruction::I32Eq,
                                        Instruction::BrIf(0),
                                        Instruction::Unreachable,
                                        Instruction::End,
                                        Instruction::Block(BlockType::NoResult),
                                        Instruction::I32Const(0x20001),
                                        Instruction::I32Const(0x63),
                                        Instruction::I32Store(2, 0),
                                        Instruction::End,
                                        Instruction::Block(BlockType::NoResult),
                                        Instruction::I32Const(0x10001),
                                        Instruction::I32Const(0x64),
                                        Instruction::I32Store(2, 0),
                                        Instruction::End,
                                        Instruction::Block(BlockType::NoResult),
                                        Instruction::I32Const(0x10001),
                                        Instruction::I32Load(2, 0),
                                        Instruction::I32Const(0x64),
                                        Instruction::I32Eq,
                                        Instruction::BrIf(0),
                                        Instruction::Unreachable,
                                        Instruction::End,
                                        Instruction::Block(BlockType::NoResult),
                                        Instruction::I32Const(0x1),
                                        Instruction::Call(1),
                                        Instruction::Drop,
                                        Instruction::End,
                                        Instruction::End,
                                    ]),
                                ),
                            ),
                        ),
                    ),
                    handle_body: Some(
                        FuncBody::new(
                            ::alloc::vec::Vec::new(),
                            Instructions::new(
                                <[_]>::into_vec(
                                    #[rustc_box]
                                    ::alloc::boxed::Box::new([
                                        Instruction::Block(BlockType::NoResult),
                                        Instruction::I32Const(0x10001),
                                        Instruction::I32Load(2, 0),
                                        Instruction::I32Const(0x0),
                                        Instruction::I32Eq,
                                        Instruction::BrIf(0),
                                        Instruction::Unreachable,
                                        Instruction::End,
                                        Instruction::Block(BlockType::NoResult),
                                        Instruction::I32Const(0x20001),
                                        Instruction::I32Load(2, 0),
                                        Instruction::I32Const(0x63),
                                        Instruction::I32Eq,
                                        Instruction::BrIf(0),
                                        Instruction::Unreachable,
                                        Instruction::End,
                                        Instruction::End,
                                    ]),
                                ),
                            ),
                        ),
                    ),
                    ..Default::default()
                }
                    .into()
            }
        }
        mod utils {
            use super::*;
            use crate::{GasAllowanceOf, SentOf};
            use frame_system::limits::BlockWeights;
            pub fn default_account<T: Origin>() -> T {
                benchmarking::account::<T>("default", 0, 0)
            }
            #[cfg(feature = "std")]
            pub fn init_logger() {
                let _ = env_logger::Builder::from_default_env()
                    .format_module_path(false)
                    .format_level(true)
                    .try_init();
            }
            /// Gets next message id, but doesn't remain changed the state of the nonces
            pub fn get_next_message_id<T>(user_id: impl Origin) -> MessageId
            where
                T: Config,
                T::AccountId: Origin,
            {
                let ret_id = Gear::<T>::next_message_id(user_id.into_origin());
                SentOf::<T>::decrease();
                ret_id
            }
            pub fn run_to_next_block<T: Config>(remaining_weight: Option<u64>)
            where
                T::AccountId: Origin,
            {
                let current_block: u32 = SystemPallet::<T>::block_number()
                    .unique_saturated_into();
                run_to_block::<T>(current_block + 1, remaining_weight);
            }
            pub fn run_to_block<T: Config>(n: u32, remaining_weight: Option<u64>)
            where
                T::AccountId: Origin,
            {
                while SystemPallet::<T>::block_number() < n.unique_saturated_into() {
                    SystemPallet::<T>::on_finalize(SystemPallet::<T>::block_number());
                    init_block::<T>(Some(SystemPallet::<T>::block_number()));
                    Gear::<T>::on_initialize(SystemPallet::<T>::block_number());
                    if let Some(remaining_weight) = remaining_weight {
                        GasAllowanceOf::<T>::put(remaining_weight);
                        let max_block_weight = <<T as frame_system::Config>::BlockWeights as Get<
                            BlockWeights,
                        >>::get()
                            .max_block;
                        SystemPallet::<
                            T,
                        >::register_extra_weight_unchecked(
                            max_block_weight
                                .saturating_sub(
                                    frame_support::weights::Weight::from_parts(
                                        remaining_weight,
                                        0,
                                    ),
                                ),
                            frame_support::dispatch::DispatchClass::Normal,
                        );
                    }
                    Gear::<T>::run(frame_support::dispatch::RawOrigin::None.into())
                        .unwrap();
                    Gear::<T>::on_finalize(SystemPallet::<T>::block_number());
                }
            }
        }
    }
    use tests::syscalls_integrity;
    use self::{
        code::{
            body::{self, DynInstr::*},
            max_pages, ImportedMemory, Location, ModuleDefinition, TableSegment,
            WasmModule, OFFSET_AUX,
        },
        sandbox::Sandbox,
    };
    use crate::{
        manager::ExtManager, pallet,
        schedule::{API_BENCHMARK_BATCH_SIZE, INSTR_BENCHMARK_BATCH_SIZE},
        BalanceOf, BenchmarkStorage, Call, Config, Event, ExecutionEnvironment,
        Ext as Externalities, GasHandlerOf, MailboxOf, Pallet as Gear, Pallet,
        ProgramStorageOf, QueueOf, RentFreePeriodOf, ResumeMinimalPeriodOf, Schedule,
    };
    use ::alloc::{
        collections::{BTreeMap, BTreeSet},
        vec,
    };
    use common::{
        self, benchmarking, paused_program_storage::SessionId, storage::{Counter, *},
        ActiveProgram, CodeMetadata, CodeStorage, GasPrice, GasTree, Origin,
        PausedProgramStorage, ProgramStorage, ReservableTree,
    };
    use core_processor::{
        common::{DispatchOutcome, JournalNote},
        configs::{BlockConfig, PageCosts, TESTS_MAX_PAGES_NUMBER},
        ProcessExecutionContext, ProcessorContext, ProcessorExternalities,
    };
    use frame_benchmarking::{benchmarks, whitelisted_caller};
    use frame_support::{
        codec::Encode, traits::{Currency, Get, Hooks, ReservableCurrency},
    };
    use frame_system::{Pallet as SystemPallet, RawOrigin};
    use gear_backend_common::Environment;
    use gear_backend_sandbox::memory::MemoryWrap;
    use gear_core::{
        code::{Code, CodeAndId},
        gas::{GasAllowanceCounter, GasCounter, ValueCounter},
        ids::{CodeId, MessageId, ProgramId},
        memory::{
            AllocationsContext, GearPage, Memory, PageBuf, PageU32Size, WasmPage,
            GEAR_PAGE_SIZE, WASM_PAGE_SIZE,
        },
        message::{ContextSettings, DispatchKind, MessageContext},
        reservation::GasReserver,
    };
    use gear_wasm_instrument::{
        parity_wasm::elements::{
            BlockType, BrTableData, Instruction, SignExtInstruction, ValueType,
        },
        syscalls::SysCallName,
    };
    use pallet_authorship::Pallet as AuthorshipPallet;
    use sp_consensus_babe::{
        digests::{PreDigest, SecondaryPlainPreDigest},
        Slot, BABE_ENGINE_ID,
    };
    use sp_core::H256;
    use sp_runtime::{
        traits::{Bounded, CheckedAdd, One, UniqueSaturatedInto, Zero},
        Digest, DigestItem, Perbill,
    };
    use sp_sandbox::{default_executor::Memory as DefaultExecutorMemory, SandboxMemory};
    use sp_std::prelude::*;
    const MAX_PAYLOAD_LEN: u32 = 32 * 64 * 1024;
    const MAX_PAYLOAD_LEN_KB: u32 = MAX_PAYLOAD_LEN / 1024;
    const MAX_PAGES: u32 = 512;
    /// How many batches we do per API benchmark.
    const API_BENCHMARK_BATCHES: u32 = 20;
    /// How many batches we do per Instruction benchmark.
    const INSTR_BENCHMARK_BATCHES: u32 = 50;
    fn init_block<T: Config>(previous: Option<T::BlockNumber>)
    where
        T::AccountId: Origin,
    {
        let slot = Slot::from(0);
        let pre_digest = Digest {
            logs: <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    DigestItem::PreRuntime(
                        BABE_ENGINE_ID,
                        PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                                slot,
                                authority_index: 0,
                            })
                            .encode(),
                    ),
                ]),
            ),
        };
        let bn = previous
            .unwrap_or_else(Zero::zero)
            .checked_add(&One::one())
            .expect("overflow");
        SystemPallet::<
            T,
        >::initialize(&bn, &SystemPallet::<T>::parent_hash(), &pre_digest);
        SystemPallet::<T>::set_block_number(bn);
        SystemPallet::<T>::on_initialize(bn);
        AuthorshipPallet::<T>::on_initialize(bn);
    }
    fn process_queue<T: Config>()
    where
        T::AccountId: Origin,
    {
        init_block::<T>(None);
        Gear::<T>::process_queue(Default::default());
    }
    fn default_processor_context<T: Config>() -> ProcessorContext {
        ProcessorContext {
            gas_counter: GasCounter::new(0),
            gas_allowance_counter: GasAllowanceCounter::new(0),
            gas_reserver: GasReserver::new(
                Default::default(),
                0,
                Default::default(),
                T::ReservationsLimit::get(),
            ),
            system_reservation: None,
            value_counter: ValueCounter::new(0),
            allocations_context: AllocationsContext::new(
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            message_context: MessageContext::new(
                Default::default(),
                Default::default(),
                ContextSettings::new(0, 0, 0, 0, 0, 0),
            ),
            block_info: Default::default(),
            max_pages: TESTS_MAX_PAGES_NUMBER.into(),
            page_costs: PageCosts::new_for_tests(),
            existential_deposit: 0,
            origin: Default::default(),
            program_id: Default::default(),
            program_candidates_data: Default::default(),
            program_rents: Default::default(),
            host_fn_weights: Default::default(),
            forbidden_funcs: Default::default(),
            mailbox_threshold: 0,
            waitlist_cost: 0,
            dispatch_hold_cost: 0,
            reserve_for: 0,
            reservation: 0,
            random_data: ([0u8; 32].to_vec(), 0),
            rent_cost: 0,
        }
    }
    fn verify_process(notes: Vec<JournalNote>) {
        if !!notes.is_empty() {
            ::core::panicking::panic_fmt(
                format_args!("Journal notes cannot be empty after execution"),
            )
        }
        let mut pages_data = BTreeMap::new();
        for note in notes {
            match note {
                JournalNote::MessageDispatched {
                    outcome: DispatchOutcome::InitFailure { .. }
                    | DispatchOutcome::MessageTrap { .. },
                    ..
                } => {
                    ::core::panicking::panic_fmt(
                        format_args!("Process was not successful"),
                    )
                }
                JournalNote::UpdatePage { page_number, data, .. } => {
                    pages_data.insert(page_number, data);
                }
                _ => {}
            }
        }
    }
    fn run_process<T>(exec: Exec<T>) -> Vec<JournalNote>
    where
        T: Config,
        T::AccountId: Origin,
    {
        core_processor::process::<
            ExecutionEnvironment,
        >(&exec.block_config, exec.context, exec.random_data, exec.memory_pages)
            .unwrap_or_else(|e| ::core::panicking::panic_fmt(
                format_args!(
                    "internal error: entered unreachable code: {0}",
                    format_args!("core-processor logic invalidated: {0}", e)
                ),
            ))
    }
    fn resume_session_prepare<T: Config>(
        c: u32,
        program_id: ProgramId,
        program: ActiveProgram<T::BlockNumber>,
        caller: T::AccountId,
        memory_page: &PageBuf,
    ) -> (SessionId, Vec<(GearPage, PageBuf)>)
    where
        T::AccountId: Origin,
    {
        ProgramStorageOf::<T>::pause_program(program_id, 100u32.into()).unwrap();
        Gear::<
            T,
        >::resume_session_init(
                RawOrigin::Signed(caller).into(),
                program_id,
                program.allocations,
                CodeId::from_origin(program.code_hash),
            )
            .expect("failed to start resume session");
        let event_record = SystemPallet::<T>::events().pop().unwrap();
        let event = <<T as pallet::Config>::RuntimeEvent as From<
            _,
        >>::from(event_record.event);
        let event: Result<Event<T>, _> = event.try_into();
        let session_id = match event {
            Ok(Event::ProgramResumeSessionStarted { session_id, .. }) => session_id,
            _ => ::core::panicking::panic("internal error: entered unreachable code"),
        };
        let memory_pages = {
            let mut pages = Vec::with_capacity(c as usize);
            for i in 0..c {
                pages.push((GearPage::from(i as u16), memory_page.clone()));
            }
            pages
        };
        (session_id, memory_pages)
    }
    /// An instantiated and deployed program.
    struct Program<T: Config> {
        addr: H256,
        caller: T::AccountId,
    }
    #[automatically_derived]
    impl<T: ::core::clone::Clone + Config> ::core::clone::Clone for Program<T>
    where
        T::AccountId: ::core::clone::Clone,
    {
        #[inline]
        fn clone(&self) -> Program<T> {
            Program {
                addr: ::core::clone::Clone::clone(&self.addr),
                caller: ::core::clone::Clone::clone(&self.caller),
            }
        }
    }
    impl<T: Config> Program<T>
    where
        T: Config,
        T::AccountId: Origin,
    {
        /// Create new program and use a default account id as instantiator.
        fn new(
            module: WasmModule<T>,
            data: Vec<u8>,
        ) -> Result<Program<T>, &'static str> {
            Self::with_index(0, module, data)
        }
        /// Create new program and use an account id derived from the supplied index as instantiator.
        fn with_index(
            index: u32,
            module: WasmModule<T>,
            data: Vec<u8>,
        ) -> Result<Program<T>, &'static str> {
            Self::with_caller(
                benchmarking::account("instantiator", index, 0),
                module,
                data,
            )
        }
        /// Create new program and use the supplied `caller` as instantiator.
        fn with_caller(
            caller: T::AccountId,
            module: WasmModule<T>,
            data: Vec<u8>,
        ) -> Result<Program<T>, &'static str> {
            let value = <T as pallet::Config>::Currency::minimum_balance();
            <T as pallet::Config>::Currency::make_free_balance_be(
                &caller,
                caller_funding::<T>(),
            );
            let salt = <[_]>::into_vec(#[rustc_box] ::alloc::boxed::Box::new([0xff]));
            let addr = ProgramId::generate(module.hash, &salt).into_origin();
            Gear::<
                T,
            >::upload_program_raw(
                RawOrigin::Signed(caller.clone()).into(),
                module.code,
                salt,
                data,
                250_000_000_000,
                value,
            )?;
            process_queue::<T>();
            let result = Program { caller, addr };
            Ok(result)
        }
    }
    /// The funding that each account that either calls or instantiates programs is funded with.
    fn caller_funding<T: pallet::Config>() -> BalanceOf<T> {
        BalanceOf::<T>::max_value() / 2u32.into()
    }
    pub struct Exec<T: Config> {
        #[allow(unused)]
        ext_manager: ExtManager<T>,
        block_config: BlockConfig,
        context: ProcessExecutionContext,
        random_data: (Vec<u8>, u32),
        memory_pages: BTreeMap<GearPage, PageBuf>,
    }
    #[allow(non_camel_case_types)]
    struct check_all;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for check_all
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            syscalls_integrity::main_test::<T>();
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {};
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct check_lazy_pages_all;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for check_lazy_pages_all
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {};
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct check_syscalls_integrity;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for check_syscalls_integrity
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            syscalls_integrity::main_test::<T>();
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {};
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct check_lazy_pages_charging;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for check_lazy_pages_charging
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {};
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct check_lazy_pages_charging_special;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for check_lazy_pages_charging_special
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {};
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct check_lazy_pages_gas_exceed;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for check_lazy_pages_gas_exceed
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {};
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct db_write_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for db_write_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::c,
                        0,
                        T::Schedule::get().limits.code_len / 1024,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let c = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::c)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let data: _ = ::alloc::vec::from_elem(c as u8, 1024 * c as usize);
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        BenchmarkStorage::<T>::insert(c, data);
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct db_read_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for db_read_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::c,
                        0,
                        T::Schedule::get().limits.code_len / 1024,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let c = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::c)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let data: _ = ::alloc::vec::from_elem(c as u8, 1024 * c as usize);
            BenchmarkStorage::<T>::insert(c, data);
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        BenchmarkStorage::<T>::get(c)
                            .expect("Infallible: Key not found in storage");
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instantiate_module_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for instantiate_module_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::c,
                        0,
                        T::Schedule::get().limits.code_len / 1024,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let c = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::c)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let WasmModule { code, .. } = WasmModule::<
                T,
            >::sized(c * 1024, Location::Init);
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let ext = Externalities::new(default_processor_context::<T>());
                        ExecutionEnvironment::new(
                                ext,
                                &code,
                                DispatchKind::Init,
                                Default::default(),
                                max_pages::<T>().into(),
                            )
                            .unwrap();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct claim_value;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for claim_value
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let caller: _ = benchmarking::account("caller", 0, 0);
            <T as pallet::Config>::Currency::deposit_creating(
                &caller,
                100_000_000_000_000_u128.unique_saturated_into(),
            );
            let program_id = benchmarking::account::<T::AccountId>("program", 0, 100);
            <T as pallet::Config>::Currency::deposit_creating(
                &program_id,
                100_000_000_000_000_u128.unique_saturated_into(),
            );
            let code = benchmarking::generate_wasm2(16.into()).unwrap();
            benchmarking::set_program::<
                ProgramStorageOf<T>,
                _,
            >(ProgramId::from_origin(program_id.clone().into_origin()), code, 1.into());
            let original_message_id = MessageId::from_origin(
                benchmarking::account::<T::AccountId>("message", 0, 100).into_origin(),
            );
            let gas_limit = 50000;
            let value = 10000u32.into();
            GasHandlerOf::<T>::create(program_id.clone(), original_message_id, gas_limit)
                .expect("Failed to create gas handler");
            <T as pallet::Config>::Currency::reserve(
                    &program_id,
                    <T as pallet::Config>::GasPrice::gas_price(gas_limit) + value,
                )
                .expect("Failed to reserve");
            MailboxOf::<
                T,
            >::insert(
                    gear_core::message::StoredMessage::new(
                        original_message_id,
                        ProgramId::from_origin(program_id.into_origin()),
                        ProgramId::from_origin(caller.clone().into_origin()),
                        Default::default(),
                        value.unique_saturated_into(),
                        None,
                    ),
                    u32::MAX.unique_saturated_into(),
                )
                .expect("Error during mailbox insertion");
            init_block::<T>(None);
            let __call = Call::<T>::new_call_variant_claim_value(original_message_id);
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = RawOrigin::Signed(caller.clone()).into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            let auto_reply = QueueOf::<T>::dequeue()
                                .expect("Error in algorithm")
                                .expect("Element should be");
                            if !auto_reply.payload().is_empty() {
                                ::core::panicking::panic(
                                    "assertion failed: auto_reply.payload().is_empty()",
                                )
                            }
                            match (
                                &auto_reply
                                    .status_code()
                                    .expect("Should be")
                                    .to_le_bytes()[0],
                                &0,
                            ) {
                                (left_val, right_val) => {
                                    if !(*left_val == *right_val) {
                                        let kind = ::core::panicking::AssertKind::Eq;
                                        ::core::panicking::assert_failed(
                                            kind,
                                            &*left_val,
                                            &*right_val,
                                            ::core::option::Option::None,
                                        );
                                    }
                                }
                            };
                            if !MailboxOf::<T>::is_empty(&caller) {
                                ::core::panicking::panic(
                                    "assertion failed: MailboxOf::<T>::is_empty(&caller)",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct pay_program_rent;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for pay_program_rent
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let caller: _ = benchmarking::account("caller", 0, 0);
            <T as pallet::Config>::Currency::deposit_creating(
                &caller,
                200_000_000_000_000u128.unique_saturated_into(),
            );
            let minimum_balance = <T as pallet::Config>::Currency::minimum_balance();
            let code = benchmarking::generate_wasm2(16.into()).unwrap();
            let salt = ::alloc::vec::Vec::new();
            let program_id = ProgramId::generate(CodeId::generate(&code), &salt);
            Gear::<
                T,
            >::upload_program(
                    RawOrigin::Signed(caller.clone()).into(),
                    code,
                    salt,
                    b"init_payload".to_vec(),
                    10_000_000_000,
                    0u32.into(),
                )
                .expect("submit program failed");
            let block_count = 1_000u32.into();
            init_block::<T>(None);
            let __call = Call::<
                T,
            >::new_call_variant_pay_program_rent(program_id, block_count);
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = RawOrigin::Signed(caller.clone()).into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            let program: ActiveProgram<_> = ProgramStorageOf::<
                                T,
                            >::get_program(program_id)
                                .expect("program should exist")
                                .try_into()
                                .expect("program should be active");
                            match (
                                &program.expiration_block,
                                &(RentFreePeriodOf::<T>::get() + block_count),
                            ) {
                                (left_val, right_val) => {
                                    if !(*left_val == *right_val) {
                                        let kind = ::core::panicking::AssertKind::Eq;
                                        ::core::panicking::assert_failed(
                                            kind,
                                            &*left_val,
                                            &*right_val,
                                            ::core::option::Option::None,
                                        );
                                    }
                                }
                            };
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct resume_session_init;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for resume_session_init
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let caller: _ = benchmarking::account("caller", 0, 0);
            <T as pallet::Config>::Currency::deposit_creating(
                &caller,
                200_000_000_000_000u128.unique_saturated_into(),
            );
            let code = benchmarking::generate_wasm2(16.into()).unwrap();
            let salt = ::alloc::vec::Vec::new();
            let program_id = ProgramId::generate(CodeId::generate(&code), &salt);
            Gear::<
                T,
            >::upload_program(
                    RawOrigin::Signed(caller.clone()).into(),
                    code,
                    salt,
                    b"init_payload".to_vec(),
                    10_000_000_000,
                    0u32.into(),
                )
                .expect("submit program failed");
            init_block::<T>(None);
            let program: ActiveProgram<_> = ProgramStorageOf::<
                T,
            >::get_program(program_id)
                .expect("program should exist")
                .try_into()
                .expect("program should be active");
            ProgramStorageOf::<T>::pause_program(program_id, 100u32.into()).unwrap();
            let __call = Call::<
                T,
            >::new_call_variant_resume_session_init(
                program_id,
                program.allocations,
                CodeId::from_origin(program.code_hash),
            );
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = RawOrigin::Signed(caller.clone()).into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            if !ProgramStorageOf::<
                                T,
                            >::paused_program_exists(&program_id) {
                                ::core::panicking::panic(
                                    "assertion failed: ProgramStorageOf::<T>::paused_program_exists(&program_id)",
                                )
                            }
                            if !!Gear::<T>::is_active(program_id) {
                                ::core::panicking::panic(
                                    "assertion failed: !Gear::<T>::is_active(program_id)",
                                )
                            }
                            if !!ProgramStorageOf::<T>::program_exists(program_id) {
                                ::core::panicking::panic(
                                    "assertion failed: !ProgramStorageOf::<T>::program_exists(program_id)",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct resume_session_push;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for resume_session_push
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::c,
                        0,
                        16 * (WASM_PAGE_SIZE / GEAR_PAGE_SIZE) as u32,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let c = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::c)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let caller: _ = benchmarking::account("caller", 0, 0);
            <T as pallet::Config>::Currency::deposit_creating(
                &caller,
                200_000_000_000_000u128.unique_saturated_into(),
            );
            let code = benchmarking::generate_wasm2(16.into()).unwrap();
            let salt = ::alloc::vec::Vec::new();
            let program_id = ProgramId::generate(CodeId::generate(&code), &salt);
            Gear::<
                T,
            >::upload_program(
                    RawOrigin::Signed(caller.clone()).into(),
                    code,
                    salt,
                    b"init_payload".to_vec(),
                    10_000_000_000,
                    0u32.into(),
                )
                .expect("submit program failed");
            init_block::<T>(None);
            let program: ActiveProgram<_> = ProgramStorageOf::<
                T,
            >::get_program(program_id)
                .expect("program should exist")
                .try_into()
                .expect("program should be active");
            let memory_page = {
                let mut page = PageBuf::new_zeroed();
                page[0] = 1;
                page
            };
            let (session_id, memory_pages) = resume_session_prepare::<
                T,
            >(c, program_id, program, caller.clone(), &memory_page);
            let __call = Call::<
                T,
            >::new_call_variant_resume_session_push(session_id, memory_pages);
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = RawOrigin::Signed(caller.clone()).into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            if !match ProgramStorageOf::<
                                T,
                            >::resume_session_page_count(&session_id) {
                                Some(count) if count == c => true,
                                _ => false,
                            } {
                                ::core::panicking::panic(
                                    "assertion failed: matches!(ProgramStorageOf :: < T > :: resume_session_page_count(& session_id),\\n    Some(count) if count == c)",
                                )
                            }
                            if !ProgramStorageOf::<
                                T,
                            >::paused_program_exists(&program_id) {
                                ::core::panicking::panic(
                                    "assertion failed: ProgramStorageOf::<T>::paused_program_exists(&program_id)",
                                )
                            }
                            if !!Gear::<T>::is_active(program_id) {
                                ::core::panicking::panic(
                                    "assertion failed: !Gear::<T>::is_active(program_id)",
                                )
                            }
                            if !!ProgramStorageOf::<T>::program_exists(program_id) {
                                ::core::panicking::panic(
                                    "assertion failed: !ProgramStorageOf::<T>::program_exists(program_id)",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct resume_session_commit;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for resume_session_commit
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::c,
                        0,
                        (MAX_PAGES - 1) * (WASM_PAGE_SIZE / GEAR_PAGE_SIZE) as u32,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let c = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::c)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let caller: _ = benchmarking::account("caller", 0, 0);
            <T as pallet::Config>::Currency::deposit_creating(
                &caller,
                400_000_000_000_000u128.unique_saturated_into(),
            );
            let code = benchmarking::generate_wasm2(0.into()).unwrap();
            let salt = ::alloc::vec::Vec::new();
            let program_id = ProgramId::generate(CodeId::generate(&code), &salt);
            Gear::<
                T,
            >::upload_program(
                    RawOrigin::Signed(caller.clone()).into(),
                    code,
                    salt,
                    b"init_payload".to_vec(),
                    10_000_000_000,
                    0u32.into(),
                )
                .expect("submit program failed");
            init_block::<T>(None);
            let memory_page = {
                let mut page = PageBuf::new_zeroed();
                page[0] = 1;
                page
            };
            for i in 0..c {
                ProgramStorageOf::<
                    T,
                >::set_program_page_data(
                    program_id,
                    GearPage::from(i as u16),
                    memory_page.clone(),
                );
            }
            let program: ActiveProgram<_> = ProgramStorageOf::<
                T,
            >::update_active_program(
                    program_id,
                    |program| {
                        program
                            .pages_with_data = BTreeSet::from_iter(
                            (0..c).map(|i| GearPage::from(i as u16)),
                        );
                        let wasm_pages = (c as usize * GEAR_PAGE_SIZE) / WASM_PAGE_SIZE;
                        program
                            .allocations = BTreeSet::from_iter(
                            (0..wasm_pages).map(|i| WasmPage::from(i as u16)),
                        );
                        program.clone()
                    },
                )
                .expect("program should exist");
            let (session_id, memory_pages) = resume_session_prepare::<
                T,
            >(c, program_id, program, caller.clone(), &memory_page);
            Gear::<
                T,
            >::resume_session_push(
                    RawOrigin::Signed(caller.clone()).into(),
                    session_id,
                    memory_pages,
                )
                .expect("failed to append memory pages");
            let __call = Call::<
                T,
            >::new_call_variant_resume_session_commit(
                session_id,
                ResumeMinimalPeriodOf::<T>::get(),
            );
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = RawOrigin::Signed(caller.clone()).into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            if !ProgramStorageOf::<T>::program_exists(program_id) {
                                ::core::panicking::panic(
                                    "assertion failed: ProgramStorageOf::<T>::program_exists(program_id)",
                                )
                            }
                            if !Gear::<T>::is_active(program_id) {
                                ::core::panicking::panic(
                                    "assertion failed: Gear::<T>::is_active(program_id)",
                                )
                            }
                            if !!ProgramStorageOf::<
                                T,
                            >::paused_program_exists(&program_id) {
                                ::core::panicking::panic(
                                    "assertion failed: !ProgramStorageOf::<T>::paused_program_exists(&program_id)",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct upload_code;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for upload_code
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::c,
                        0,
                        Perbill::from_percent(49)
                            .mul_ceil(T::Schedule::get().limits.code_len) / 1024,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let c = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::c)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let value: _ = <T as pallet::Config>::Currency::minimum_balance();
            let caller = whitelisted_caller();
            <T as pallet::Config>::Currency::make_free_balance_be(
                &caller,
                caller_funding::<T>(),
            );
            let WasmModule { code, hash: code_id, .. } = WasmModule::<
                T,
            >::sized(c * 1024, Location::Handle);
            let origin = RawOrigin::Signed(caller);
            init_block::<T>(None);
            let __call = Call::<T>::new_call_variant_upload_code(code);
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = origin.into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            if !<T as pallet::Config>::CodeStorage::exists(code_id) {
                                ::core::panicking::panic(
                                    "assertion failed: <T as pallet::Config>::CodeStorage::exists(code_id)",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct create_program;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for create_program
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::s,
                        0,
                        code::max_pages::<T>() as u32 * 64 * 128,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let s = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::s)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let caller: _ = whitelisted_caller();
            let origin = RawOrigin::Signed(caller);
            let WasmModule { code, hash: code_id, .. } = WasmModule::<T>::dummy();
            Gear::<T>::upload_code(origin.into(), code).expect("submit code failed");
            let salt = ::alloc::vec::from_elem(42u8, s as usize);
            let value = <T as pallet::Config>::Currency::minimum_balance();
            let caller = whitelisted_caller();
            <T as pallet::Config>::Currency::make_free_balance_be(
                &caller,
                caller_funding::<T>(),
            );
            let origin = RawOrigin::Signed(caller);
            init_block::<T>(None);
            let __call = Call::<
                T,
            >::new_call_variant_create_program(
                code_id,
                salt,
                ::alloc::vec::Vec::new(),
                100_000_000_u64,
                value,
            );
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = origin.into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            if !<T as pallet::Config>::CodeStorage::exists(code_id) {
                                ::core::panicking::panic(
                                    "assertion failed: <T as pallet::Config>::CodeStorage::exists(code_id)",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct upload_program;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for upload_program
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::c,
                        0,
                        Perbill::from_percent(49)
                            .mul_ceil(T::Schedule::get().limits.code_len) / 1024,
                    ),
                    (
                        ::frame_benchmarking::BenchmarkParameter::s,
                        0,
                        code::max_pages::<T>() as u32 * 64 * 128,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let c = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::c)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            let s = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::s)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            ();
            let salt: _ = ::alloc::vec::from_elem(42u8, s as usize);
            let value = <T as pallet::Config>::Currency::minimum_balance();
            let caller = whitelisted_caller();
            <T as pallet::Config>::Currency::make_free_balance_be(
                &caller,
                caller_funding::<T>(),
            );
            let WasmModule { code, hash, .. } = WasmModule::<
                T,
            >::sized(c * 1024, Location::Handle);
            let origin = RawOrigin::Signed(caller);
            init_block::<T>(None);
            let __call = Call::<
                T,
            >::new_call_variant_upload_program(
                code,
                salt,
                ::alloc::vec::Vec::new(),
                100_000_000_u64,
                value,
            );
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = origin.into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            if !match QueueOf::<T>::dequeue() {
                                Ok(Some(_)) => true,
                                _ => false,
                            } {
                                ::core::panicking::panic(
                                    "assertion failed: matches!(QueueOf :: < T > :: dequeue(), Ok(Some(_)))",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct send_message;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for send_message
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::p, 0, MAX_PAYLOAD_LEN),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let caller: _ = benchmarking::account("caller", 0, 0);
            <T as pallet::Config>::Currency::deposit_creating(
                &caller,
                100_000_000_000_000_u128.unique_saturated_into(),
            );
            let minimum_balance = <T as pallet::Config>::Currency::minimum_balance();
            let program_id = ProgramId::from_origin(
                benchmarking::account::<T::AccountId>("program", 0, 100).into_origin(),
            );
            let code = benchmarking::generate_wasm2(16.into()).unwrap();
            benchmarking::set_program::<
                ProgramStorageOf<T>,
                _,
            >(program_id, code, 1.into());
            let payload = ::alloc::vec::from_elem(0_u8, p as usize);
            init_block::<T>(None);
            let __call = Call::<
                T,
            >::new_call_variant_send_message(
                program_id,
                payload,
                100_000_000_u64,
                minimum_balance,
            );
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = RawOrigin::Signed(caller).into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            if !match QueueOf::<T>::dequeue() {
                                Ok(Some(_)) => true,
                                _ => false,
                            } {
                                ::core::panicking::panic(
                                    "assertion failed: matches!(QueueOf :: < T > :: dequeue(), Ok(Some(_)))",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct send_reply;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for send_reply
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::p, 0, MAX_PAYLOAD_LEN),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let caller: _ = benchmarking::account("caller", 0, 0);
            <T as pallet::Config>::Currency::deposit_creating(
                &caller,
                100_000_000_000_000_u128.unique_saturated_into(),
            );
            let minimum_balance = <T as pallet::Config>::Currency::minimum_balance();
            let program_id = benchmarking::account::<T::AccountId>("program", 0, 100);
            <T as pallet::Config>::Currency::deposit_creating(
                &program_id,
                100_000_000_000_000_u128.unique_saturated_into(),
            );
            let code = benchmarking::generate_wasm2(16.into()).unwrap();
            benchmarking::set_program::<
                ProgramStorageOf<T>,
                _,
            >(ProgramId::from_origin(program_id.clone().into_origin()), code, 1.into());
            let original_message_id = MessageId::from_origin(
                benchmarking::account::<T::AccountId>("message", 0, 100).into_origin(),
            );
            let gas_limit = 50000;
            let value = (p % 2).into();
            GasHandlerOf::<T>::create(program_id.clone(), original_message_id, gas_limit)
                .expect("Failed to create gas handler");
            <T as pallet::Config>::Currency::reserve(
                    &program_id,
                    <T as pallet::Config>::GasPrice::gas_price(gas_limit) + value,
                )
                .expect("Failed to reserve");
            MailboxOf::<
                T,
            >::insert(
                    gear_core::message::StoredMessage::new(
                        original_message_id,
                        ProgramId::from_origin(program_id.into_origin()),
                        ProgramId::from_origin(caller.clone().into_origin()),
                        Default::default(),
                        value.unique_saturated_into(),
                        None,
                    ),
                    u32::MAX.unique_saturated_into(),
                )
                .expect("Error during mailbox insertion");
            let payload = ::alloc::vec::from_elem(0_u8, p as usize);
            init_block::<T>(None);
            let __call = Call::<
                T,
            >::new_call_variant_send_reply(
                original_message_id,
                payload,
                100_000_000_u64,
                minimum_balance,
            );
            let __benchmarked_call_encoded = ::frame_benchmarking::frame_support::codec::Encode::encode(
                &__call,
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let __call_decoded = <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::codec::Decode>::decode(
                                &mut &__benchmarked_call_encoded[..],
                            )
                            .expect("call is encoded above, encoding must be correct");
                        let __origin = RawOrigin::Signed(caller.clone()).into();
                        <Call<
                            T,
                        > as ::frame_benchmarking::frame_support::traits::UnfilteredDispatchable>::dispatch_bypass_filter(
                            __call_decoded,
                            __origin,
                        )?;
                    };
                    if verify {
                        {
                            if !match QueueOf::<T>::dequeue() {
                                Ok(Some(_)) => true,
                                _ => false,
                            } {
                                ::core::panicking::panic(
                                    "assertion failed: matches!(QueueOf :: < T > :: dequeue(), Ok(Some(_)))",
                                )
                            }
                            if !MailboxOf::<T>::is_empty(&caller) {
                                ::core::panicking::panic(
                                    "assertion failed: MailboxOf::<T>::is_empty(&caller)",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct initial_allocation;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for initial_allocation
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::q, 1, MAX_PAGES),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let q = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::q)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let q: _ = q as u16;
            let caller: T::AccountId = benchmarking::account("caller", 0, 0);
            <T as pallet::Config>::Currency::deposit_creating(
                &caller,
                (1u128 << 60).unique_saturated_into(),
            );
            let code = benchmarking::generate_wasm(q.into()).unwrap();
            let salt = ::alloc::vec::from_elem(255u8, 32);
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let _ = Gear::<
                            T,
                        >::upload_program(
                            RawOrigin::Signed(caller).into(),
                            code,
                            salt,
                            ::alloc::vec::Vec::new(),
                            100_000_000u64,
                            0u32.into(),
                        );
                        process_queue::<T>();
                    };
                    if verify {
                        {
                            if !match QueueOf::<T>::dequeue() {
                                Ok(None) => true,
                                _ => false,
                            } {
                                ::core::panicking::panic(
                                    "assertion failed: matches!(QueueOf :: < T > :: dequeue(), Ok(None))",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct alloc_in_handle;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for alloc_in_handle
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::q, 0, MAX_PAGES),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let q = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::q)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let q: _ = q as u16;
            let caller: T::AccountId = benchmarking::account("caller", 0, 0);
            <T as pallet::Config>::Currency::deposit_creating(
                &caller,
                (1_u128 << 60).unique_saturated_into(),
            );
            let code = benchmarking::generate_wasm2(q.into()).unwrap();
            let salt = ::alloc::vec::from_elem(255u8, 32);
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        let _ = Gear::<
                            T,
                        >::upload_program(
                            RawOrigin::Signed(caller).into(),
                            code,
                            salt,
                            ::alloc::vec::Vec::new(),
                            100_000_000u64,
                            0u32.into(),
                        );
                        process_queue::<T>();
                    };
                    if verify {
                        {
                            if !match QueueOf::<T>::dequeue() {
                                Ok(None) => true,
                                _ => false,
                            } {
                                ::core::panicking::panic(
                                    "assertion failed: matches!(QueueOf :: < T > :: dequeue(), Ok(None))",
                                )
                            }
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct reinstrument_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for reinstrument_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::c,
                        0,
                        T::Schedule::get().limits.code_len / 1_024,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let c = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::c)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let WasmModule { code, hash, .. } = WasmModule::<
                T,
            >::sized(c * 1_024, Location::Handle);
            let code = Code::new_raw(code, 1, None, false, true).unwrap();
            let code_and_id = CodeAndId::new(code);
            let code_id = code_and_id.code_id();
            let caller: T::AccountId = benchmarking::account("caller", 0, 0);
            let metadata = {
                let block_number = Pallet::<T>::block_number().unique_saturated_into();
                CodeMetadata::new(caller.into_origin(), block_number)
            };
            T::CodeStorage::add_code(code_and_id, metadata).unwrap();
            let schedule = T::Schedule::get();
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        Gear::<T>::reinstrument_code(code_id, &schedule);
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct alloc;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for alloc
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::alloc(r, 1)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct alloc_per_page;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for alloc_per_page
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::p, 1, 25),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::alloc(1, p)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct free;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for free
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::free(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reserve_gas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reserve_gas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        T::ReservationsLimit::get() as u32,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reserve_gas(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_unreserve_gas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_unreserve_gas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        T::ReservationsLimit::get() as u32,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_unreserve_gas(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_system_reserve_gas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_system_reserve_gas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_system_reserve_gas(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_message_id;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_message_id
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::MessageId, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_origin;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_origin
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::Origin, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_program_id;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_program_id
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::ProgramId, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_source;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_source
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::Source, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_value;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_value
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::Value, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_value_available;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_value_available
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::ValueAvailable, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_gas_available;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_gas_available
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::GasAvailable, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_size;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_size
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::Size, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_read;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_read
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_read(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_read_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_read_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_read_per_kb(n)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_block_height;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_block_height
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::BlockHeight, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_block_timestamp;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_block_timestamp
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::getter(SysCallName::BlockTimestamp, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_random;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_random
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::n,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_random(n)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_deposit;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_deposit
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_deposit(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send(r, None, false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send(1, Some(n), false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_wgas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_wgas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send(r, None, true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_wgas_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_wgas_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send(1, Some(n), true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_input;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_input
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send_input(r, None, false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_input_wgas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_input_wgas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send_input(r, None, true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_init;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_init
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send_init(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_push;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_push
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send_push(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_push_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_push_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send_push_per_kb(n)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_commit;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_commit
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send_commit(r, false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_commit_wgas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_commit_wgas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send_commit(r, true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reservation_send;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reservation_send
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reservation_send(r, None)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reservation_send_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for gr_reservation_send_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reservation_send(1, Some(n))?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reservation_send_commit;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for gr_reservation_send_commit
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reservation_send_commit(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply(r, None, false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply(1, Some(n), false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_wgas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_wgas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply(r, None, true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_wgas_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_wgas_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply(1, Some(n), true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_commit;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_commit
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_commit(r, false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_commit_wgas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_commit_wgas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_commit(r, true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_push;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_push
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_push(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_push_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_push_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::n,
                        0,
                        gear_core::message::MAX_PAYLOAD_SIZE as u32 / 1024,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_push_per_kb(n)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_input;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_input
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_input(r, None, false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_input_wgas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_input_wgas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_input(r, None, true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reservation_reply;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reservation_reply
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reservation_reply(r, None)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reservation_reply_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for gr_reservation_reply_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reservation_reply(1, Some(n))?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reservation_reply_commit;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for gr_reservation_reply_commit
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reservation_reply_commit(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reservation_reply_commit_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for gr_reservation_reply_commit_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reservation_reply_commit_per_kb(n)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_to;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_to
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_to(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_signal_from;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_signal_from
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_signal_from(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_push_input;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_reply_push_input
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_push_input(Some(r), None)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_reply_push_input_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for gr_reply_push_input_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_reply_push_input(None, Some(n))?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_push_input;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_send_push_input
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send_push_input(r, None)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_send_push_input_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for gr_send_push_input_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_send_push_input(1, Some(n))?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_debug;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_debug
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_debug(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_debug_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_debug_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::n, 0, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let n = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::n)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_debug_per_kb(n)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_error;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_error
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_error(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_status_code;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_status_code
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_status_code(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_exit;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_exit
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<
                T,
            >::termination_bench(SysCallName::Exit, Some(0xff), r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_leave;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_leave
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::termination_bench(SysCallName::Leave, None, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_wait;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_wait
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::termination_bench(SysCallName::Wait, None, r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_wait_for;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_wait_for
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<
                T,
            >::termination_bench(SysCallName::WaitFor, Some(10), r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_wait_up_to;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_wait_up_to
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::r, 0, 1),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<
                T,
            >::termination_bench(SysCallName::WaitUpTo, Some(100), r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_wake;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_wake
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_wake(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_create_program;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_create_program
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_create_program(r, None, None, false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_create_program_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for gr_create_program_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::p, 0, MAX_PAYLOAD_LEN_KB),
                    (::frame_benchmarking::BenchmarkParameter::s, 1, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            let s = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::s)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_create_program(1, Some(p), Some(s), false)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_create_program_wgas;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_create_program_wgas
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_create_program(r, None, None, true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_create_program_wgas_per_kb;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for gr_create_program_wgas_per_kb
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (::frame_benchmarking::BenchmarkParameter::p, 0, MAX_PAYLOAD_LEN_KB),
                    (::frame_benchmarking::BenchmarkParameter::s, 1, MAX_PAYLOAD_LEN_KB),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            let s = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::s)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_create_program(1, Some(p), Some(s), true)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct gr_pay_program_rent;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for gr_pay_program_rent
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::gr_pay_program_rent(r)?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct lazy_pages_signal_read;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for lazy_pages_signal_read
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::p,
                        0,
                        code::max_pages::<T>() as u32,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::lazy_pages_signal_read((p as u16).into())?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct lazy_pages_signal_write;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for lazy_pages_signal_write
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::p,
                        0,
                        code::max_pages::<T>() as u32,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::lazy_pages_signal_write((p as u16).into())?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct lazy_pages_signal_write_after_read;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for lazy_pages_signal_write_after_read
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::p,
                        0,
                        code::max_pages::<T>() as u32,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<
                T,
            >::lazy_pages_signal_write_after_read((p as u16).into())?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct lazy_pages_load_page_storage_data;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for lazy_pages_load_page_storage_data
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::p,
                        0,
                        code::max_pages::<T>() as u32,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<
                T,
            >::lazy_pages_load_page_storage_data((p as u16).into())?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct lazy_pages_host_func_read;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for lazy_pages_host_func_read
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::p,
                        0,
                        MAX_PAYLOAD_LEN / WasmPage::size(),
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::lazy_pages_host_func_read((p as u16).into())?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct lazy_pages_host_func_write;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for lazy_pages_host_func_write
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::p,
                        0,
                        MAX_PAYLOAD_LEN / WasmPage::size(),
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<T>::lazy_pages_host_func_write((p as u16).into())?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct lazy_pages_host_func_write_after_read;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for lazy_pages_host_func_write_after_read
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::p,
                        0,
                        MAX_PAYLOAD_LEN / WasmPage::size(),
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut res = None;
            let exec = Benches::<
                T,
            >::lazy_pages_host_func_write_after_read((p as u16).into())?;
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        res.replace(run_process(exec));
                    };
                    if verify {
                        {
                            verify_process(res.unwrap());
                        };
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct mem_grow;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for mem_grow
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        API_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut mem = MemoryWrap::new(DefaultExecutorMemory::new(1, None).unwrap());
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        for _ in 0..(r * API_BENCHMARK_BATCH_SIZE) {
                            mem.grow(1.into()).unwrap();
                        }
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64load;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64load
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        INSTR_BENCHMARK_BATCHES,
                        10 * INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mem_pages: _ = code::max_pages::<T>();
            let module = ModuleDefinition {
                memory: Some(ImportedMemory::new(mem_pages)),
                handle_body: Some(
                    body::repeated_dyn(
                        r * INSTR_BENCHMARK_BATCH_SIZE,
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                RandomUnaligned(0, mem_pages as u32 * WasmPage::size() - 8),
                                Regular(Instruction::I64Load(3, 0)),
                                Regular(Instruction::Drop),
                            ]),
                        ),
                    ),
                ),
                ..Default::default()
            };
            let mut sbox = Sandbox::from_module_def::<T>(module);
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32load;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32load
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        INSTR_BENCHMARK_BATCHES,
                        10 * INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mem_pages: _ = code::max_pages::<T>();
            let module = ModuleDefinition {
                memory: Some(ImportedMemory::new(mem_pages)),
                handle_body: Some(
                    body::repeated_dyn(
                        r * INSTR_BENCHMARK_BATCH_SIZE,
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                RandomUnaligned(0, mem_pages as u32 * WasmPage::size() - 4),
                                Regular(Instruction::I32Load(2, 0)),
                                Regular(Instruction::Drop),
                            ]),
                        ),
                    ),
                ),
                ..Default::default()
            };
            let mut sbox = Sandbox::from_module_def::<T>(module);
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64store;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64store
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        INSTR_BENCHMARK_BATCHES,
                        10 * INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mem_pages: _ = code::max_pages::<T>();
            let module = ModuleDefinition {
                memory: Some(ImportedMemory::new(mem_pages)),
                handle_body: Some(
                    body::repeated_dyn(
                        r * INSTR_BENCHMARK_BATCH_SIZE,
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                RandomUnaligned(0, mem_pages as u32 * WasmPage::size() - 8),
                                RandomI64Repeated(1),
                                Regular(Instruction::I64Store(3, 0)),
                            ]),
                        ),
                    ),
                ),
                ..Default::default()
            };
            let mut sbox = Sandbox::from_module_def::<T>(module);
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32store;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32store
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        INSTR_BENCHMARK_BATCHES,
                        10 * INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mem_pages: _ = code::max_pages::<T>();
            let module = ModuleDefinition {
                memory: Some(ImportedMemory::new(mem_pages)),
                handle_body: Some(
                    body::repeated_dyn(
                        r * INSTR_BENCHMARK_BATCH_SIZE,
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                RandomUnaligned(0, mem_pages as u32 * WasmPage::size() - 4),
                                RandomI32Repeated(1),
                                Regular(Instruction::I32Store(2, 0)),
                            ]),
                        ),
                    ),
                ),
                ..Default::default()
            };
            let mut sbox = Sandbox::from_module_def::<T>(module);
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_select;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_select
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI64Repeated(1),
                                    RandomI64Repeated(1),
                                    RandomI32(0, 2),
                                    Regular(Instruction::Select),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_if;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_if
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut instructions = body::repeated_dyn_instr(
                r * INSTR_BENCHMARK_BATCH_SIZE,
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        Regular(Instruction::If(BlockType::Value(ValueType::I32))),
                        RandomI32Repeated(1),
                        Regular(Instruction::Else),
                        RandomI32Repeated(1),
                        Regular(Instruction::End),
                    ]),
                ),
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([Instruction::I32Const(1)]),
                ),
            );
            instructions.push(Instruction::Drop);
            let body = body::from_instructions(instructions);
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(body),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_br;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_br
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Regular(Instruction::Block(BlockType::NoResult)),
                                    Regular(Instruction::Br(0)),
                                    Regular(Instruction::End),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_br_if;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_br_if
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Regular(Instruction::Block(BlockType::NoResult)),
                                    RandomI32(0, 2),
                                    Regular(Instruction::BrIf(0)),
                                    Regular(Instruction::End),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_br_table;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_br_table
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let table: _ = Box::new(BrTableData {
                table: Box::new([0]),
                default: 0,
            });
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Regular(Instruction::Block(BlockType::NoResult)),
                                    RandomI32Repeated(1),
                                    Regular(Instruction::BrTable(table)),
                                    Regular(Instruction::End),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_br_table_per_entry;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for instr_br_table_per_entry
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::e,
                        1,
                        T::Schedule::get().limits.br_table_size,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let e = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::e)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let entry: Vec<u32> = [0, 1]
                .iter()
                .cloned()
                .cycle()
                .take((e / 2) as usize)
                .collect();
            let table = Box::new(BrTableData {
                table: entry.into_boxed_slice(),
                default: 0,
            });
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Regular(Instruction::Block(BlockType::NoResult)),
                                    Regular(Instruction::Block(BlockType::NoResult)),
                                    Regular(Instruction::Block(BlockType::NoResult)),
                                    RandomI32(0, (e + 1) as i32),
                                    Regular(Instruction::BrTable(table)),
                                    RandomI64Repeated(1),
                                    Regular(Instruction::Drop),
                                    Regular(Instruction::End),
                                    RandomI64Repeated(1),
                                    Regular(Instruction::Drop),
                                    Regular(Instruction::End),
                                    RandomI64Repeated(1),
                                    Regular(Instruction::Drop),
                                    Regular(Instruction::End),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_call_const;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_call_const
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    aux_body: Some(
                        body::from_instructions(
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    Instruction::I64Const(0x7ffffffff3ffffff),
                                ]),
                            ),
                        ),
                    ),
                    aux_res: Some(ValueType::I64),
                    handle_body: Some(
                        body::repeated(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            &[Instruction::Call(OFFSET_AUX), Instruction::Drop],
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_call;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_call
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    aux_body: Some(body::empty()),
                    handle_body: Some(
                        body::repeated(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            &[Instruction::Call(OFFSET_AUX)],
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_call_indirect;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_call_indirect
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let num_elements: _ = T::Schedule::get().limits.table_size;
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    aux_body: Some(body::empty()),
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI32(0, num_elements as i32),
                                    Regular(Instruction::CallIndirect(0, 0)),
                                ]),
                            ),
                        ),
                    ),
                    table: Some(TableSegment {
                        num_elements,
                        function_index: OFFSET_AUX,
                    }),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_call_indirect_per_param;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T>
    for instr_call_indirect_per_param
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::p,
                        0,
                        T::Schedule::get().limits.parameters,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let p = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::p)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let num_elements: _ = T::Schedule::get().limits.table_size;
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    aux_body: Some(body::empty()),
                    aux_arg_num: p,
                    handle_body: Some(
                        body::repeated_dyn(
                            INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI64Repeated(p as usize),
                                    RandomI32(0, num_elements as i32),
                                    Regular(Instruction::CallIndirect(p.min(1), 0)),
                                ]),
                            ),
                        ),
                    ),
                    table: Some(TableSegment {
                        num_elements,
                        function_index: OFFSET_AUX,
                    }),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_call_per_local;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_call_per_local
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::l,
                        0,
                        T::Schedule::get().limits.locals,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let l = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::l)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut aux_body = body::empty();
            body::inject_locals(&mut aux_body, l);
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    aux_body: Some(aux_body),
                    handle_body: Some(
                        body::repeated(
                            INSTR_BENCHMARK_BATCH_SIZE,
                            &[Instruction::Call(2)],
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_local_get;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_local_get
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let max_locals: _ = T::Schedule::get().limits.stack_height.unwrap_or(512);
            let mut handle_body = body::repeated_dyn(
                r * INSTR_BENCHMARK_BATCH_SIZE,
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        RandomGetLocal(0, max_locals),
                        Regular(Instruction::Drop),
                    ]),
                ),
            );
            body::inject_locals(&mut handle_body, max_locals);
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(handle_body),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_local_set;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_local_set
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let max_locals: _ = T::Schedule::get().limits.stack_height.unwrap_or(512);
            let mut handle_body = body::repeated_dyn(
                r * INSTR_BENCHMARK_BATCH_SIZE,
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        RandomI64Repeated(1),
                        RandomSetLocal(0, max_locals),
                    ]),
                ),
            );
            body::inject_locals(&mut handle_body, max_locals);
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(handle_body),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_local_tee;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_local_tee
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let max_locals: _ = T::Schedule::get().limits.stack_height.unwrap_or(512);
            let mut handle_body = body::repeated_dyn(
                r * INSTR_BENCHMARK_BATCH_SIZE,
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        RandomI64Repeated(1),
                        RandomTeeLocal(0, max_locals),
                        Regular(Instruction::Drop),
                    ]),
                ),
            );
            body::inject_locals(&mut handle_body, max_locals);
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(handle_body),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_global_get;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_global_get
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let max_globals: _ = T::Schedule::get().limits.globals;
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomGetGlobal(0, max_globals),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    num_globals: max_globals,
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_global_set;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_global_set
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let max_globals: _ = T::Schedule::get().limits.globals;
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI64Repeated(1),
                                    RandomSetGlobal(0, max_globals),
                                ]),
                            ),
                        ),
                    ),
                    num_globals: max_globals,
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_memory_current;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_memory_current
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    memory: Some(ImportedMemory::max::<T>()),
                    handle_body: Some(
                        body::repeated(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            &[Instruction::CurrentMemory(0), Instruction::Drop],
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64clz;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64clz
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::unary_instr_64(Instruction::I64Clz, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32clz;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32clz
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::unary_instr_32(Instruction::I32Clz, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64ctz;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64ctz
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::unary_instr_64(Instruction::I64Ctz, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32ctz;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32ctz
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::unary_instr_32(Instruction::I32Ctz, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64popcnt;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64popcnt
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::unary_instr_64(Instruction::I64Popcnt, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32popcnt;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32popcnt
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::unary_instr_32(Instruction::I32Popcnt, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64eqz;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64eqz
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::unary_instr_64(Instruction::I64Eqz, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32eqz;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32eqz
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::unary_instr_32(Instruction::I32Eqz, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32extend8s;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32extend8s
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI32Repeated(1),
                                    Regular(
                                        Instruction::SignExt(SignExtInstruction::I32Extend8S),
                                    ),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32extend16s;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32extend16s
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI32Repeated(1),
                                    Regular(
                                        Instruction::SignExt(SignExtInstruction::I32Extend16S),
                                    ),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64extend8s;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64extend8s
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI64Repeated(1),
                                    Regular(
                                        Instruction::SignExt(SignExtInstruction::I64Extend8S),
                                    ),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64extend16s;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64extend16s
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI64Repeated(1),
                                    Regular(
                                        Instruction::SignExt(SignExtInstruction::I64Extend16S),
                                    ),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64extend32s;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64extend32s
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI64Repeated(1),
                                    Regular(
                                        Instruction::SignExt(SignExtInstruction::I64Extend32S),
                                    ),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64extendsi32;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64extendsi32
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI32Repeated(1),
                                    Regular(Instruction::I64ExtendSI32),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64extendui32;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64extendui32
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::from(ModuleDefinition {
                    handle_body: Some(
                        body::repeated_dyn(
                            r * INSTR_BENCHMARK_BATCH_SIZE,
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    RandomI32Repeated(1),
                                    Regular(Instruction::I64ExtendUI32),
                                    Regular(Instruction::Drop),
                                ]),
                            ),
                        ),
                    ),
                    ..Default::default()
                }),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32wrapi64;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32wrapi64
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::unary_instr_64(
                    Instruction::I32WrapI64,
                    r * INSTR_BENCHMARK_BATCH_SIZE,
                ),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64eq;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64eq
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Eq, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32eq;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32eq
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Eq, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64ne;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64ne
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Ne, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32ne;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32ne
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Ne, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64lts;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64lts
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64LtS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32lts;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32lts
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32LtS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64ltu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64ltu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64LtU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32ltu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32ltu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32LtU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64gts;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64gts
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64GtS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32gts;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32gts
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32GtS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64gtu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64gtu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64GtU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32gtu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32gtu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32GtU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64les;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64les
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64LeS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32les;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32les
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32LeS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64leu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64leu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64LeU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32leu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32leu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32LeU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64ges;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64ges
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64GeS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32ges;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32ges
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32GeS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64geu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64geu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64GeU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32geu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32geu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32GeU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64add;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64add
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Add, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32add;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32add
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Add, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64sub;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64sub
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Sub, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32sub;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32sub
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Sub, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64mul;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64mul
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Mul, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32mul;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32mul
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Mul, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64divs;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64divs
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64DivS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32divs;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32divs
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32DivS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64divu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64divu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64DivU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32divu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32divu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32DivU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64rems;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64rems
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64RemS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32rems;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32rems
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32RemS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64remu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64remu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64RemU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32remu;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32remu
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32RemU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64and;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64and
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64And, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32and;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32and
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32And, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64or;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64or
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Or, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32or;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32or
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Or, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64xor;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64xor
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Xor, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32xor;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32xor
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Xor, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64shl;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64shl
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Shl, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32shl;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32shl
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Shl, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64shrs;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64shrs
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64ShrS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32shrs;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32shrs
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32ShrS, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64shru;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64shru
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64ShrU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32shru;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32shru
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32ShrU, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64rotl;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64rotl
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Rotl, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32rotl;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32rotl
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Rotl, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i64rotr;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i64rotr
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_64(Instruction::I64Rotr, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct instr_i32rotr;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for instr_i32rotr
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (
                        ::frame_benchmarking::BenchmarkParameter::r,
                        0,
                        INSTR_BENCHMARK_BATCHES,
                    ),
                ]),
            )
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            let r = components
                .iter()
                .find(|&c| c.0 == ::frame_benchmarking::BenchmarkParameter::r)
                .ok_or("Could not find component in benchmark preparation.")?
                .1;
            ();
            let mut sbox = Sandbox::from(
                &WasmModule::<
                    T,
                >::binary_instr_32(Instruction::I32Rotr, r * INSTR_BENCHMARK_BATCH_SIZE),
            );
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {
                        sbox.invoke();
                    };
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    struct print_schedule;
    #[allow(unused_variables)]
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for print_schedule
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            ::alloc::vec::Vec::new()
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            #[cfg(feature = "std")]
            {
                {
                    ::std::io::_print(
                        format_args!("{0:#?}\n", Schedule::< T >::default()),
                    );
                };
            }
            Ok(
                ::frame_benchmarking::Box::new(move || -> Result<
                    (),
                    ::frame_benchmarking::BenchmarkError,
                > {
                    {};
                    if verify {
                        {};
                    }
                    Ok(())
                }),
            )
        }
    }
    #[allow(non_camel_case_types)]
    enum SelectedBenchmark {
        check_all,
        check_lazy_pages_all,
        check_syscalls_integrity,
        check_lazy_pages_charging,
        check_lazy_pages_charging_special,
        check_lazy_pages_gas_exceed,
        db_write_per_kb,
        db_read_per_kb,
        instantiate_module_per_kb,
        claim_value,
        pay_program_rent,
        resume_session_init,
        resume_session_push,
        resume_session_commit,
        upload_code,
        create_program,
        upload_program,
        send_message,
        send_reply,
        initial_allocation,
        alloc_in_handle,
        reinstrument_per_kb,
        alloc,
        alloc_per_page,
        free,
        gr_reserve_gas,
        gr_unreserve_gas,
        gr_system_reserve_gas,
        gr_message_id,
        gr_origin,
        gr_program_id,
        gr_source,
        gr_value,
        gr_value_available,
        gr_gas_available,
        gr_size,
        gr_read,
        gr_read_per_kb,
        gr_block_height,
        gr_block_timestamp,
        gr_random,
        gr_reply_deposit,
        gr_send,
        gr_send_per_kb,
        gr_send_wgas,
        gr_send_wgas_per_kb,
        gr_send_input,
        gr_send_input_wgas,
        gr_send_init,
        gr_send_push,
        gr_send_push_per_kb,
        gr_send_commit,
        gr_send_commit_wgas,
        gr_reservation_send,
        gr_reservation_send_per_kb,
        gr_reservation_send_commit,
        gr_reply,
        gr_reply_per_kb,
        gr_reply_wgas,
        gr_reply_wgas_per_kb,
        gr_reply_commit,
        gr_reply_commit_wgas,
        gr_reply_push,
        gr_reply_push_per_kb,
        gr_reply_input,
        gr_reply_input_wgas,
        gr_reservation_reply,
        gr_reservation_reply_per_kb,
        gr_reservation_reply_commit,
        gr_reservation_reply_commit_per_kb,
        gr_reply_to,
        gr_signal_from,
        gr_reply_push_input,
        gr_reply_push_input_per_kb,
        gr_send_push_input,
        gr_send_push_input_per_kb,
        gr_debug,
        gr_debug_per_kb,
        gr_error,
        gr_status_code,
        gr_exit,
        gr_leave,
        gr_wait,
        gr_wait_for,
        gr_wait_up_to,
        gr_wake,
        gr_create_program,
        gr_create_program_per_kb,
        gr_create_program_wgas,
        gr_create_program_wgas_per_kb,
        gr_pay_program_rent,
        lazy_pages_signal_read,
        lazy_pages_signal_write,
        lazy_pages_signal_write_after_read,
        lazy_pages_load_page_storage_data,
        lazy_pages_host_func_read,
        lazy_pages_host_func_write,
        lazy_pages_host_func_write_after_read,
        mem_grow,
        instr_i64load,
        instr_i32load,
        instr_i64store,
        instr_i32store,
        instr_select,
        instr_if,
        instr_br,
        instr_br_if,
        instr_br_table,
        instr_br_table_per_entry,
        instr_call_const,
        instr_call,
        instr_call_indirect,
        instr_call_indirect_per_param,
        instr_call_per_local,
        instr_local_get,
        instr_local_set,
        instr_local_tee,
        instr_global_get,
        instr_global_set,
        instr_memory_current,
        instr_i64clz,
        instr_i32clz,
        instr_i64ctz,
        instr_i32ctz,
        instr_i64popcnt,
        instr_i32popcnt,
        instr_i64eqz,
        instr_i32eqz,
        instr_i32extend8s,
        instr_i32extend16s,
        instr_i64extend8s,
        instr_i64extend16s,
        instr_i64extend32s,
        instr_i64extendsi32,
        instr_i64extendui32,
        instr_i32wrapi64,
        instr_i64eq,
        instr_i32eq,
        instr_i64ne,
        instr_i32ne,
        instr_i64lts,
        instr_i32lts,
        instr_i64ltu,
        instr_i32ltu,
        instr_i64gts,
        instr_i32gts,
        instr_i64gtu,
        instr_i32gtu,
        instr_i64les,
        instr_i32les,
        instr_i64leu,
        instr_i32leu,
        instr_i64ges,
        instr_i32ges,
        instr_i64geu,
        instr_i32geu,
        instr_i64add,
        instr_i32add,
        instr_i64sub,
        instr_i32sub,
        instr_i64mul,
        instr_i32mul,
        instr_i64divs,
        instr_i32divs,
        instr_i64divu,
        instr_i32divu,
        instr_i64rems,
        instr_i32rems,
        instr_i64remu,
        instr_i32remu,
        instr_i64and,
        instr_i32and,
        instr_i64or,
        instr_i32or,
        instr_i64xor,
        instr_i32xor,
        instr_i64shl,
        instr_i32shl,
        instr_i64shrs,
        instr_i32shrs,
        instr_i64shru,
        instr_i32shru,
        instr_i64rotl,
        instr_i32rotl,
        instr_i64rotr,
        instr_i32rotr,
        print_schedule,
    }
    impl<T: Config> ::frame_benchmarking::BenchmarkingSetup<T> for SelectedBenchmark
    where
        T::AccountId: Origin,
    {
        fn components(
            &self,
        ) -> ::frame_benchmarking::Vec<
            (::frame_benchmarking::BenchmarkParameter, u32, u32),
        > {
            match self {
                Self::check_all => {
                    <check_all as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&check_all)
                }
                Self::check_lazy_pages_all => {
                    <check_lazy_pages_all as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&check_lazy_pages_all)
                }
                Self::check_syscalls_integrity => {
                    <check_syscalls_integrity as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&check_syscalls_integrity)
                }
                Self::check_lazy_pages_charging => {
                    <check_lazy_pages_charging as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&check_lazy_pages_charging)
                }
                Self::check_lazy_pages_charging_special => {
                    <check_lazy_pages_charging_special as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&check_lazy_pages_charging_special)
                }
                Self::check_lazy_pages_gas_exceed => {
                    <check_lazy_pages_gas_exceed as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&check_lazy_pages_gas_exceed)
                }
                Self::db_write_per_kb => {
                    <db_write_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&db_write_per_kb)
                }
                Self::db_read_per_kb => {
                    <db_read_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&db_read_per_kb)
                }
                Self::instantiate_module_per_kb => {
                    <instantiate_module_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instantiate_module_per_kb)
                }
                Self::claim_value => {
                    <claim_value as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&claim_value)
                }
                Self::pay_program_rent => {
                    <pay_program_rent as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&pay_program_rent)
                }
                Self::resume_session_init => {
                    <resume_session_init as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&resume_session_init)
                }
                Self::resume_session_push => {
                    <resume_session_push as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&resume_session_push)
                }
                Self::resume_session_commit => {
                    <resume_session_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&resume_session_commit)
                }
                Self::upload_code => {
                    <upload_code as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&upload_code)
                }
                Self::create_program => {
                    <create_program as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&create_program)
                }
                Self::upload_program => {
                    <upload_program as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&upload_program)
                }
                Self::send_message => {
                    <send_message as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&send_message)
                }
                Self::send_reply => {
                    <send_reply as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&send_reply)
                }
                Self::initial_allocation => {
                    <initial_allocation as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&initial_allocation)
                }
                Self::alloc_in_handle => {
                    <alloc_in_handle as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&alloc_in_handle)
                }
                Self::reinstrument_per_kb => {
                    <reinstrument_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&reinstrument_per_kb)
                }
                Self::alloc => {
                    <alloc as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&alloc)
                }
                Self::alloc_per_page => {
                    <alloc_per_page as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&alloc_per_page)
                }
                Self::free => {
                    <free as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&free)
                }
                Self::gr_reserve_gas => {
                    <gr_reserve_gas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reserve_gas)
                }
                Self::gr_unreserve_gas => {
                    <gr_unreserve_gas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_unreserve_gas)
                }
                Self::gr_system_reserve_gas => {
                    <gr_system_reserve_gas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_system_reserve_gas)
                }
                Self::gr_message_id => {
                    <gr_message_id as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_message_id)
                }
                Self::gr_origin => {
                    <gr_origin as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_origin)
                }
                Self::gr_program_id => {
                    <gr_program_id as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_program_id)
                }
                Self::gr_source => {
                    <gr_source as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_source)
                }
                Self::gr_value => {
                    <gr_value as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_value)
                }
                Self::gr_value_available => {
                    <gr_value_available as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_value_available)
                }
                Self::gr_gas_available => {
                    <gr_gas_available as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_gas_available)
                }
                Self::gr_size => {
                    <gr_size as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_size)
                }
                Self::gr_read => {
                    <gr_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_read)
                }
                Self::gr_read_per_kb => {
                    <gr_read_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_read_per_kb)
                }
                Self::gr_block_height => {
                    <gr_block_height as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_block_height)
                }
                Self::gr_block_timestamp => {
                    <gr_block_timestamp as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_block_timestamp)
                }
                Self::gr_random => {
                    <gr_random as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_random)
                }
                Self::gr_reply_deposit => {
                    <gr_reply_deposit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_deposit)
                }
                Self::gr_send => {
                    <gr_send as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send)
                }
                Self::gr_send_per_kb => {
                    <gr_send_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_per_kb)
                }
                Self::gr_send_wgas => {
                    <gr_send_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_wgas)
                }
                Self::gr_send_wgas_per_kb => {
                    <gr_send_wgas_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_wgas_per_kb)
                }
                Self::gr_send_input => {
                    <gr_send_input as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_input)
                }
                Self::gr_send_input_wgas => {
                    <gr_send_input_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_input_wgas)
                }
                Self::gr_send_init => {
                    <gr_send_init as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_init)
                }
                Self::gr_send_push => {
                    <gr_send_push as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_push)
                }
                Self::gr_send_push_per_kb => {
                    <gr_send_push_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_push_per_kb)
                }
                Self::gr_send_commit => {
                    <gr_send_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_commit)
                }
                Self::gr_send_commit_wgas => {
                    <gr_send_commit_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_commit_wgas)
                }
                Self::gr_reservation_send => {
                    <gr_reservation_send as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reservation_send)
                }
                Self::gr_reservation_send_per_kb => {
                    <gr_reservation_send_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reservation_send_per_kb)
                }
                Self::gr_reservation_send_commit => {
                    <gr_reservation_send_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reservation_send_commit)
                }
                Self::gr_reply => {
                    <gr_reply as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply)
                }
                Self::gr_reply_per_kb => {
                    <gr_reply_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_per_kb)
                }
                Self::gr_reply_wgas => {
                    <gr_reply_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_wgas)
                }
                Self::gr_reply_wgas_per_kb => {
                    <gr_reply_wgas_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_wgas_per_kb)
                }
                Self::gr_reply_commit => {
                    <gr_reply_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_commit)
                }
                Self::gr_reply_commit_wgas => {
                    <gr_reply_commit_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_commit_wgas)
                }
                Self::gr_reply_push => {
                    <gr_reply_push as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_push)
                }
                Self::gr_reply_push_per_kb => {
                    <gr_reply_push_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_push_per_kb)
                }
                Self::gr_reply_input => {
                    <gr_reply_input as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_input)
                }
                Self::gr_reply_input_wgas => {
                    <gr_reply_input_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_input_wgas)
                }
                Self::gr_reservation_reply => {
                    <gr_reservation_reply as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reservation_reply)
                }
                Self::gr_reservation_reply_per_kb => {
                    <gr_reservation_reply_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reservation_reply_per_kb)
                }
                Self::gr_reservation_reply_commit => {
                    <gr_reservation_reply_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reservation_reply_commit)
                }
                Self::gr_reservation_reply_commit_per_kb => {
                    <gr_reservation_reply_commit_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reservation_reply_commit_per_kb)
                }
                Self::gr_reply_to => {
                    <gr_reply_to as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_to)
                }
                Self::gr_signal_from => {
                    <gr_signal_from as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_signal_from)
                }
                Self::gr_reply_push_input => {
                    <gr_reply_push_input as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_push_input)
                }
                Self::gr_reply_push_input_per_kb => {
                    <gr_reply_push_input_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_reply_push_input_per_kb)
                }
                Self::gr_send_push_input => {
                    <gr_send_push_input as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_push_input)
                }
                Self::gr_send_push_input_per_kb => {
                    <gr_send_push_input_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_send_push_input_per_kb)
                }
                Self::gr_debug => {
                    <gr_debug as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_debug)
                }
                Self::gr_debug_per_kb => {
                    <gr_debug_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_debug_per_kb)
                }
                Self::gr_error => {
                    <gr_error as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_error)
                }
                Self::gr_status_code => {
                    <gr_status_code as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_status_code)
                }
                Self::gr_exit => {
                    <gr_exit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_exit)
                }
                Self::gr_leave => {
                    <gr_leave as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_leave)
                }
                Self::gr_wait => {
                    <gr_wait as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_wait)
                }
                Self::gr_wait_for => {
                    <gr_wait_for as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_wait_for)
                }
                Self::gr_wait_up_to => {
                    <gr_wait_up_to as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_wait_up_to)
                }
                Self::gr_wake => {
                    <gr_wake as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_wake)
                }
                Self::gr_create_program => {
                    <gr_create_program as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_create_program)
                }
                Self::gr_create_program_per_kb => {
                    <gr_create_program_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_create_program_per_kb)
                }
                Self::gr_create_program_wgas => {
                    <gr_create_program_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_create_program_wgas)
                }
                Self::gr_create_program_wgas_per_kb => {
                    <gr_create_program_wgas_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_create_program_wgas_per_kb)
                }
                Self::gr_pay_program_rent => {
                    <gr_pay_program_rent as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&gr_pay_program_rent)
                }
                Self::lazy_pages_signal_read => {
                    <lazy_pages_signal_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&lazy_pages_signal_read)
                }
                Self::lazy_pages_signal_write => {
                    <lazy_pages_signal_write as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&lazy_pages_signal_write)
                }
                Self::lazy_pages_signal_write_after_read => {
                    <lazy_pages_signal_write_after_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&lazy_pages_signal_write_after_read)
                }
                Self::lazy_pages_load_page_storage_data => {
                    <lazy_pages_load_page_storage_data as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&lazy_pages_load_page_storage_data)
                }
                Self::lazy_pages_host_func_read => {
                    <lazy_pages_host_func_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&lazy_pages_host_func_read)
                }
                Self::lazy_pages_host_func_write => {
                    <lazy_pages_host_func_write as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&lazy_pages_host_func_write)
                }
                Self::lazy_pages_host_func_write_after_read => {
                    <lazy_pages_host_func_write_after_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&lazy_pages_host_func_write_after_read)
                }
                Self::mem_grow => {
                    <mem_grow as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&mem_grow)
                }
                Self::instr_i64load => {
                    <instr_i64load as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64load)
                }
                Self::instr_i32load => {
                    <instr_i32load as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32load)
                }
                Self::instr_i64store => {
                    <instr_i64store as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64store)
                }
                Self::instr_i32store => {
                    <instr_i32store as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32store)
                }
                Self::instr_select => {
                    <instr_select as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_select)
                }
                Self::instr_if => {
                    <instr_if as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_if)
                }
                Self::instr_br => {
                    <instr_br as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_br)
                }
                Self::instr_br_if => {
                    <instr_br_if as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_br_if)
                }
                Self::instr_br_table => {
                    <instr_br_table as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_br_table)
                }
                Self::instr_br_table_per_entry => {
                    <instr_br_table_per_entry as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_br_table_per_entry)
                }
                Self::instr_call_const => {
                    <instr_call_const as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_call_const)
                }
                Self::instr_call => {
                    <instr_call as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_call)
                }
                Self::instr_call_indirect => {
                    <instr_call_indirect as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_call_indirect)
                }
                Self::instr_call_indirect_per_param => {
                    <instr_call_indirect_per_param as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_call_indirect_per_param)
                }
                Self::instr_call_per_local => {
                    <instr_call_per_local as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_call_per_local)
                }
                Self::instr_local_get => {
                    <instr_local_get as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_local_get)
                }
                Self::instr_local_set => {
                    <instr_local_set as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_local_set)
                }
                Self::instr_local_tee => {
                    <instr_local_tee as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_local_tee)
                }
                Self::instr_global_get => {
                    <instr_global_get as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_global_get)
                }
                Self::instr_global_set => {
                    <instr_global_set as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_global_set)
                }
                Self::instr_memory_current => {
                    <instr_memory_current as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_memory_current)
                }
                Self::instr_i64clz => {
                    <instr_i64clz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64clz)
                }
                Self::instr_i32clz => {
                    <instr_i32clz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32clz)
                }
                Self::instr_i64ctz => {
                    <instr_i64ctz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64ctz)
                }
                Self::instr_i32ctz => {
                    <instr_i32ctz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32ctz)
                }
                Self::instr_i64popcnt => {
                    <instr_i64popcnt as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64popcnt)
                }
                Self::instr_i32popcnt => {
                    <instr_i32popcnt as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32popcnt)
                }
                Self::instr_i64eqz => {
                    <instr_i64eqz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64eqz)
                }
                Self::instr_i32eqz => {
                    <instr_i32eqz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32eqz)
                }
                Self::instr_i32extend8s => {
                    <instr_i32extend8s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32extend8s)
                }
                Self::instr_i32extend16s => {
                    <instr_i32extend16s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32extend16s)
                }
                Self::instr_i64extend8s => {
                    <instr_i64extend8s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64extend8s)
                }
                Self::instr_i64extend16s => {
                    <instr_i64extend16s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64extend16s)
                }
                Self::instr_i64extend32s => {
                    <instr_i64extend32s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64extend32s)
                }
                Self::instr_i64extendsi32 => {
                    <instr_i64extendsi32 as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64extendsi32)
                }
                Self::instr_i64extendui32 => {
                    <instr_i64extendui32 as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64extendui32)
                }
                Self::instr_i32wrapi64 => {
                    <instr_i32wrapi64 as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32wrapi64)
                }
                Self::instr_i64eq => {
                    <instr_i64eq as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64eq)
                }
                Self::instr_i32eq => {
                    <instr_i32eq as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32eq)
                }
                Self::instr_i64ne => {
                    <instr_i64ne as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64ne)
                }
                Self::instr_i32ne => {
                    <instr_i32ne as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32ne)
                }
                Self::instr_i64lts => {
                    <instr_i64lts as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64lts)
                }
                Self::instr_i32lts => {
                    <instr_i32lts as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32lts)
                }
                Self::instr_i64ltu => {
                    <instr_i64ltu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64ltu)
                }
                Self::instr_i32ltu => {
                    <instr_i32ltu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32ltu)
                }
                Self::instr_i64gts => {
                    <instr_i64gts as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64gts)
                }
                Self::instr_i32gts => {
                    <instr_i32gts as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32gts)
                }
                Self::instr_i64gtu => {
                    <instr_i64gtu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64gtu)
                }
                Self::instr_i32gtu => {
                    <instr_i32gtu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32gtu)
                }
                Self::instr_i64les => {
                    <instr_i64les as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64les)
                }
                Self::instr_i32les => {
                    <instr_i32les as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32les)
                }
                Self::instr_i64leu => {
                    <instr_i64leu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64leu)
                }
                Self::instr_i32leu => {
                    <instr_i32leu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32leu)
                }
                Self::instr_i64ges => {
                    <instr_i64ges as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64ges)
                }
                Self::instr_i32ges => {
                    <instr_i32ges as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32ges)
                }
                Self::instr_i64geu => {
                    <instr_i64geu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64geu)
                }
                Self::instr_i32geu => {
                    <instr_i32geu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32geu)
                }
                Self::instr_i64add => {
                    <instr_i64add as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64add)
                }
                Self::instr_i32add => {
                    <instr_i32add as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32add)
                }
                Self::instr_i64sub => {
                    <instr_i64sub as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64sub)
                }
                Self::instr_i32sub => {
                    <instr_i32sub as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32sub)
                }
                Self::instr_i64mul => {
                    <instr_i64mul as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64mul)
                }
                Self::instr_i32mul => {
                    <instr_i32mul as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32mul)
                }
                Self::instr_i64divs => {
                    <instr_i64divs as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64divs)
                }
                Self::instr_i32divs => {
                    <instr_i32divs as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32divs)
                }
                Self::instr_i64divu => {
                    <instr_i64divu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64divu)
                }
                Self::instr_i32divu => {
                    <instr_i32divu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32divu)
                }
                Self::instr_i64rems => {
                    <instr_i64rems as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64rems)
                }
                Self::instr_i32rems => {
                    <instr_i32rems as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32rems)
                }
                Self::instr_i64remu => {
                    <instr_i64remu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64remu)
                }
                Self::instr_i32remu => {
                    <instr_i32remu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32remu)
                }
                Self::instr_i64and => {
                    <instr_i64and as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64and)
                }
                Self::instr_i32and => {
                    <instr_i32and as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32and)
                }
                Self::instr_i64or => {
                    <instr_i64or as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64or)
                }
                Self::instr_i32or => {
                    <instr_i32or as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32or)
                }
                Self::instr_i64xor => {
                    <instr_i64xor as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64xor)
                }
                Self::instr_i32xor => {
                    <instr_i32xor as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32xor)
                }
                Self::instr_i64shl => {
                    <instr_i64shl as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64shl)
                }
                Self::instr_i32shl => {
                    <instr_i32shl as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32shl)
                }
                Self::instr_i64shrs => {
                    <instr_i64shrs as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64shrs)
                }
                Self::instr_i32shrs => {
                    <instr_i32shrs as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32shrs)
                }
                Self::instr_i64shru => {
                    <instr_i64shru as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64shru)
                }
                Self::instr_i32shru => {
                    <instr_i32shru as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32shru)
                }
                Self::instr_i64rotl => {
                    <instr_i64rotl as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64rotl)
                }
                Self::instr_i32rotl => {
                    <instr_i32rotl as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32rotl)
                }
                Self::instr_i64rotr => {
                    <instr_i64rotr as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i64rotr)
                }
                Self::instr_i32rotr => {
                    <instr_i32rotr as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&instr_i32rotr)
                }
                Self::print_schedule => {
                    <print_schedule as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&print_schedule)
                }
            }
        }
        fn instance(
            &self,
            components: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            verify: bool,
        ) -> Result<
            ::frame_benchmarking::Box<
                dyn FnOnce() -> Result<(), ::frame_benchmarking::BenchmarkError>,
            >,
            ::frame_benchmarking::BenchmarkError,
        > {
            match self {
                Self::check_all => {
                    <check_all as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&check_all, components, verify)
                }
                Self::check_lazy_pages_all => {
                    <check_lazy_pages_all as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&check_lazy_pages_all, components, verify)
                }
                Self::check_syscalls_integrity => {
                    <check_syscalls_integrity as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&check_syscalls_integrity, components, verify)
                }
                Self::check_lazy_pages_charging => {
                    <check_lazy_pages_charging as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&check_lazy_pages_charging, components, verify)
                }
                Self::check_lazy_pages_charging_special => {
                    <check_lazy_pages_charging_special as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&check_lazy_pages_charging_special, components, verify)
                }
                Self::check_lazy_pages_gas_exceed => {
                    <check_lazy_pages_gas_exceed as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&check_lazy_pages_gas_exceed, components, verify)
                }
                Self::db_write_per_kb => {
                    <db_write_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&db_write_per_kb, components, verify)
                }
                Self::db_read_per_kb => {
                    <db_read_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&db_read_per_kb, components, verify)
                }
                Self::instantiate_module_per_kb => {
                    <instantiate_module_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instantiate_module_per_kb, components, verify)
                }
                Self::claim_value => {
                    <claim_value as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&claim_value, components, verify)
                }
                Self::pay_program_rent => {
                    <pay_program_rent as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&pay_program_rent, components, verify)
                }
                Self::resume_session_init => {
                    <resume_session_init as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&resume_session_init, components, verify)
                }
                Self::resume_session_push => {
                    <resume_session_push as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&resume_session_push, components, verify)
                }
                Self::resume_session_commit => {
                    <resume_session_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&resume_session_commit, components, verify)
                }
                Self::upload_code => {
                    <upload_code as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&upload_code, components, verify)
                }
                Self::create_program => {
                    <create_program as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&create_program, components, verify)
                }
                Self::upload_program => {
                    <upload_program as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&upload_program, components, verify)
                }
                Self::send_message => {
                    <send_message as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&send_message, components, verify)
                }
                Self::send_reply => {
                    <send_reply as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&send_reply, components, verify)
                }
                Self::initial_allocation => {
                    <initial_allocation as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&initial_allocation, components, verify)
                }
                Self::alloc_in_handle => {
                    <alloc_in_handle as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&alloc_in_handle, components, verify)
                }
                Self::reinstrument_per_kb => {
                    <reinstrument_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&reinstrument_per_kb, components, verify)
                }
                Self::alloc => {
                    <alloc as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&alloc, components, verify)
                }
                Self::alloc_per_page => {
                    <alloc_per_page as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&alloc_per_page, components, verify)
                }
                Self::free => {
                    <free as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&free, components, verify)
                }
                Self::gr_reserve_gas => {
                    <gr_reserve_gas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reserve_gas, components, verify)
                }
                Self::gr_unreserve_gas => {
                    <gr_unreserve_gas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_unreserve_gas, components, verify)
                }
                Self::gr_system_reserve_gas => {
                    <gr_system_reserve_gas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_system_reserve_gas, components, verify)
                }
                Self::gr_message_id => {
                    <gr_message_id as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_message_id, components, verify)
                }
                Self::gr_origin => {
                    <gr_origin as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_origin, components, verify)
                }
                Self::gr_program_id => {
                    <gr_program_id as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_program_id, components, verify)
                }
                Self::gr_source => {
                    <gr_source as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_source, components, verify)
                }
                Self::gr_value => {
                    <gr_value as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_value, components, verify)
                }
                Self::gr_value_available => {
                    <gr_value_available as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_value_available, components, verify)
                }
                Self::gr_gas_available => {
                    <gr_gas_available as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_gas_available, components, verify)
                }
                Self::gr_size => {
                    <gr_size as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_size, components, verify)
                }
                Self::gr_read => {
                    <gr_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_read, components, verify)
                }
                Self::gr_read_per_kb => {
                    <gr_read_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_read_per_kb, components, verify)
                }
                Self::gr_block_height => {
                    <gr_block_height as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_block_height, components, verify)
                }
                Self::gr_block_timestamp => {
                    <gr_block_timestamp as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_block_timestamp, components, verify)
                }
                Self::gr_random => {
                    <gr_random as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_random, components, verify)
                }
                Self::gr_reply_deposit => {
                    <gr_reply_deposit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_deposit, components, verify)
                }
                Self::gr_send => {
                    <gr_send as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send, components, verify)
                }
                Self::gr_send_per_kb => {
                    <gr_send_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_per_kb, components, verify)
                }
                Self::gr_send_wgas => {
                    <gr_send_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_wgas, components, verify)
                }
                Self::gr_send_wgas_per_kb => {
                    <gr_send_wgas_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_wgas_per_kb, components, verify)
                }
                Self::gr_send_input => {
                    <gr_send_input as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_input, components, verify)
                }
                Self::gr_send_input_wgas => {
                    <gr_send_input_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_input_wgas, components, verify)
                }
                Self::gr_send_init => {
                    <gr_send_init as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_init, components, verify)
                }
                Self::gr_send_push => {
                    <gr_send_push as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_push, components, verify)
                }
                Self::gr_send_push_per_kb => {
                    <gr_send_push_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_push_per_kb, components, verify)
                }
                Self::gr_send_commit => {
                    <gr_send_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_commit, components, verify)
                }
                Self::gr_send_commit_wgas => {
                    <gr_send_commit_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_commit_wgas, components, verify)
                }
                Self::gr_reservation_send => {
                    <gr_reservation_send as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reservation_send, components, verify)
                }
                Self::gr_reservation_send_per_kb => {
                    <gr_reservation_send_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reservation_send_per_kb, components, verify)
                }
                Self::gr_reservation_send_commit => {
                    <gr_reservation_send_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reservation_send_commit, components, verify)
                }
                Self::gr_reply => {
                    <gr_reply as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply, components, verify)
                }
                Self::gr_reply_per_kb => {
                    <gr_reply_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_per_kb, components, verify)
                }
                Self::gr_reply_wgas => {
                    <gr_reply_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_wgas, components, verify)
                }
                Self::gr_reply_wgas_per_kb => {
                    <gr_reply_wgas_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_wgas_per_kb, components, verify)
                }
                Self::gr_reply_commit => {
                    <gr_reply_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_commit, components, verify)
                }
                Self::gr_reply_commit_wgas => {
                    <gr_reply_commit_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_commit_wgas, components, verify)
                }
                Self::gr_reply_push => {
                    <gr_reply_push as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_push, components, verify)
                }
                Self::gr_reply_push_per_kb => {
                    <gr_reply_push_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_push_per_kb, components, verify)
                }
                Self::gr_reply_input => {
                    <gr_reply_input as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_input, components, verify)
                }
                Self::gr_reply_input_wgas => {
                    <gr_reply_input_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_input_wgas, components, verify)
                }
                Self::gr_reservation_reply => {
                    <gr_reservation_reply as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reservation_reply, components, verify)
                }
                Self::gr_reservation_reply_per_kb => {
                    <gr_reservation_reply_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reservation_reply_per_kb, components, verify)
                }
                Self::gr_reservation_reply_commit => {
                    <gr_reservation_reply_commit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reservation_reply_commit, components, verify)
                }
                Self::gr_reservation_reply_commit_per_kb => {
                    <gr_reservation_reply_commit_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reservation_reply_commit_per_kb, components, verify)
                }
                Self::gr_reply_to => {
                    <gr_reply_to as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_to, components, verify)
                }
                Self::gr_signal_from => {
                    <gr_signal_from as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_signal_from, components, verify)
                }
                Self::gr_reply_push_input => {
                    <gr_reply_push_input as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_push_input, components, verify)
                }
                Self::gr_reply_push_input_per_kb => {
                    <gr_reply_push_input_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_reply_push_input_per_kb, components, verify)
                }
                Self::gr_send_push_input => {
                    <gr_send_push_input as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_push_input, components, verify)
                }
                Self::gr_send_push_input_per_kb => {
                    <gr_send_push_input_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_send_push_input_per_kb, components, verify)
                }
                Self::gr_debug => {
                    <gr_debug as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_debug, components, verify)
                }
                Self::gr_debug_per_kb => {
                    <gr_debug_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_debug_per_kb, components, verify)
                }
                Self::gr_error => {
                    <gr_error as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_error, components, verify)
                }
                Self::gr_status_code => {
                    <gr_status_code as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_status_code, components, verify)
                }
                Self::gr_exit => {
                    <gr_exit as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_exit, components, verify)
                }
                Self::gr_leave => {
                    <gr_leave as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_leave, components, verify)
                }
                Self::gr_wait => {
                    <gr_wait as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_wait, components, verify)
                }
                Self::gr_wait_for => {
                    <gr_wait_for as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_wait_for, components, verify)
                }
                Self::gr_wait_up_to => {
                    <gr_wait_up_to as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_wait_up_to, components, verify)
                }
                Self::gr_wake => {
                    <gr_wake as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_wake, components, verify)
                }
                Self::gr_create_program => {
                    <gr_create_program as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_create_program, components, verify)
                }
                Self::gr_create_program_per_kb => {
                    <gr_create_program_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_create_program_per_kb, components, verify)
                }
                Self::gr_create_program_wgas => {
                    <gr_create_program_wgas as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_create_program_wgas, components, verify)
                }
                Self::gr_create_program_wgas_per_kb => {
                    <gr_create_program_wgas_per_kb as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_create_program_wgas_per_kb, components, verify)
                }
                Self::gr_pay_program_rent => {
                    <gr_pay_program_rent as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&gr_pay_program_rent, components, verify)
                }
                Self::lazy_pages_signal_read => {
                    <lazy_pages_signal_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&lazy_pages_signal_read, components, verify)
                }
                Self::lazy_pages_signal_write => {
                    <lazy_pages_signal_write as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&lazy_pages_signal_write, components, verify)
                }
                Self::lazy_pages_signal_write_after_read => {
                    <lazy_pages_signal_write_after_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&lazy_pages_signal_write_after_read, components, verify)
                }
                Self::lazy_pages_load_page_storage_data => {
                    <lazy_pages_load_page_storage_data as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&lazy_pages_load_page_storage_data, components, verify)
                }
                Self::lazy_pages_host_func_read => {
                    <lazy_pages_host_func_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&lazy_pages_host_func_read, components, verify)
                }
                Self::lazy_pages_host_func_write => {
                    <lazy_pages_host_func_write as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&lazy_pages_host_func_write, components, verify)
                }
                Self::lazy_pages_host_func_write_after_read => {
                    <lazy_pages_host_func_write_after_read as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(
                        &lazy_pages_host_func_write_after_read,
                        components,
                        verify,
                    )
                }
                Self::mem_grow => {
                    <mem_grow as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&mem_grow, components, verify)
                }
                Self::instr_i64load => {
                    <instr_i64load as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64load, components, verify)
                }
                Self::instr_i32load => {
                    <instr_i32load as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32load, components, verify)
                }
                Self::instr_i64store => {
                    <instr_i64store as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64store, components, verify)
                }
                Self::instr_i32store => {
                    <instr_i32store as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32store, components, verify)
                }
                Self::instr_select => {
                    <instr_select as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_select, components, verify)
                }
                Self::instr_if => {
                    <instr_if as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_if, components, verify)
                }
                Self::instr_br => {
                    <instr_br as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_br, components, verify)
                }
                Self::instr_br_if => {
                    <instr_br_if as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_br_if, components, verify)
                }
                Self::instr_br_table => {
                    <instr_br_table as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_br_table, components, verify)
                }
                Self::instr_br_table_per_entry => {
                    <instr_br_table_per_entry as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_br_table_per_entry, components, verify)
                }
                Self::instr_call_const => {
                    <instr_call_const as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_call_const, components, verify)
                }
                Self::instr_call => {
                    <instr_call as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_call, components, verify)
                }
                Self::instr_call_indirect => {
                    <instr_call_indirect as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_call_indirect, components, verify)
                }
                Self::instr_call_indirect_per_param => {
                    <instr_call_indirect_per_param as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_call_indirect_per_param, components, verify)
                }
                Self::instr_call_per_local => {
                    <instr_call_per_local as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_call_per_local, components, verify)
                }
                Self::instr_local_get => {
                    <instr_local_get as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_local_get, components, verify)
                }
                Self::instr_local_set => {
                    <instr_local_set as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_local_set, components, verify)
                }
                Self::instr_local_tee => {
                    <instr_local_tee as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_local_tee, components, verify)
                }
                Self::instr_global_get => {
                    <instr_global_get as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_global_get, components, verify)
                }
                Self::instr_global_set => {
                    <instr_global_set as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_global_set, components, verify)
                }
                Self::instr_memory_current => {
                    <instr_memory_current as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_memory_current, components, verify)
                }
                Self::instr_i64clz => {
                    <instr_i64clz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64clz, components, verify)
                }
                Self::instr_i32clz => {
                    <instr_i32clz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32clz, components, verify)
                }
                Self::instr_i64ctz => {
                    <instr_i64ctz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64ctz, components, verify)
                }
                Self::instr_i32ctz => {
                    <instr_i32ctz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32ctz, components, verify)
                }
                Self::instr_i64popcnt => {
                    <instr_i64popcnt as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64popcnt, components, verify)
                }
                Self::instr_i32popcnt => {
                    <instr_i32popcnt as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32popcnt, components, verify)
                }
                Self::instr_i64eqz => {
                    <instr_i64eqz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64eqz, components, verify)
                }
                Self::instr_i32eqz => {
                    <instr_i32eqz as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32eqz, components, verify)
                }
                Self::instr_i32extend8s => {
                    <instr_i32extend8s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32extend8s, components, verify)
                }
                Self::instr_i32extend16s => {
                    <instr_i32extend16s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32extend16s, components, verify)
                }
                Self::instr_i64extend8s => {
                    <instr_i64extend8s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64extend8s, components, verify)
                }
                Self::instr_i64extend16s => {
                    <instr_i64extend16s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64extend16s, components, verify)
                }
                Self::instr_i64extend32s => {
                    <instr_i64extend32s as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64extend32s, components, verify)
                }
                Self::instr_i64extendsi32 => {
                    <instr_i64extendsi32 as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64extendsi32, components, verify)
                }
                Self::instr_i64extendui32 => {
                    <instr_i64extendui32 as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64extendui32, components, verify)
                }
                Self::instr_i32wrapi64 => {
                    <instr_i32wrapi64 as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32wrapi64, components, verify)
                }
                Self::instr_i64eq => {
                    <instr_i64eq as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64eq, components, verify)
                }
                Self::instr_i32eq => {
                    <instr_i32eq as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32eq, components, verify)
                }
                Self::instr_i64ne => {
                    <instr_i64ne as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64ne, components, verify)
                }
                Self::instr_i32ne => {
                    <instr_i32ne as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32ne, components, verify)
                }
                Self::instr_i64lts => {
                    <instr_i64lts as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64lts, components, verify)
                }
                Self::instr_i32lts => {
                    <instr_i32lts as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32lts, components, verify)
                }
                Self::instr_i64ltu => {
                    <instr_i64ltu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64ltu, components, verify)
                }
                Self::instr_i32ltu => {
                    <instr_i32ltu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32ltu, components, verify)
                }
                Self::instr_i64gts => {
                    <instr_i64gts as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64gts, components, verify)
                }
                Self::instr_i32gts => {
                    <instr_i32gts as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32gts, components, verify)
                }
                Self::instr_i64gtu => {
                    <instr_i64gtu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64gtu, components, verify)
                }
                Self::instr_i32gtu => {
                    <instr_i32gtu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32gtu, components, verify)
                }
                Self::instr_i64les => {
                    <instr_i64les as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64les, components, verify)
                }
                Self::instr_i32les => {
                    <instr_i32les as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32les, components, verify)
                }
                Self::instr_i64leu => {
                    <instr_i64leu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64leu, components, verify)
                }
                Self::instr_i32leu => {
                    <instr_i32leu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32leu, components, verify)
                }
                Self::instr_i64ges => {
                    <instr_i64ges as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64ges, components, verify)
                }
                Self::instr_i32ges => {
                    <instr_i32ges as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32ges, components, verify)
                }
                Self::instr_i64geu => {
                    <instr_i64geu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64geu, components, verify)
                }
                Self::instr_i32geu => {
                    <instr_i32geu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32geu, components, verify)
                }
                Self::instr_i64add => {
                    <instr_i64add as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64add, components, verify)
                }
                Self::instr_i32add => {
                    <instr_i32add as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32add, components, verify)
                }
                Self::instr_i64sub => {
                    <instr_i64sub as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64sub, components, verify)
                }
                Self::instr_i32sub => {
                    <instr_i32sub as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32sub, components, verify)
                }
                Self::instr_i64mul => {
                    <instr_i64mul as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64mul, components, verify)
                }
                Self::instr_i32mul => {
                    <instr_i32mul as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32mul, components, verify)
                }
                Self::instr_i64divs => {
                    <instr_i64divs as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64divs, components, verify)
                }
                Self::instr_i32divs => {
                    <instr_i32divs as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32divs, components, verify)
                }
                Self::instr_i64divu => {
                    <instr_i64divu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64divu, components, verify)
                }
                Self::instr_i32divu => {
                    <instr_i32divu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32divu, components, verify)
                }
                Self::instr_i64rems => {
                    <instr_i64rems as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64rems, components, verify)
                }
                Self::instr_i32rems => {
                    <instr_i32rems as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32rems, components, verify)
                }
                Self::instr_i64remu => {
                    <instr_i64remu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64remu, components, verify)
                }
                Self::instr_i32remu => {
                    <instr_i32remu as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32remu, components, verify)
                }
                Self::instr_i64and => {
                    <instr_i64and as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64and, components, verify)
                }
                Self::instr_i32and => {
                    <instr_i32and as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32and, components, verify)
                }
                Self::instr_i64or => {
                    <instr_i64or as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64or, components, verify)
                }
                Self::instr_i32or => {
                    <instr_i32or as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32or, components, verify)
                }
                Self::instr_i64xor => {
                    <instr_i64xor as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64xor, components, verify)
                }
                Self::instr_i32xor => {
                    <instr_i32xor as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32xor, components, verify)
                }
                Self::instr_i64shl => {
                    <instr_i64shl as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64shl, components, verify)
                }
                Self::instr_i32shl => {
                    <instr_i32shl as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32shl, components, verify)
                }
                Self::instr_i64shrs => {
                    <instr_i64shrs as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64shrs, components, verify)
                }
                Self::instr_i32shrs => {
                    <instr_i32shrs as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32shrs, components, verify)
                }
                Self::instr_i64shru => {
                    <instr_i64shru as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64shru, components, verify)
                }
                Self::instr_i32shru => {
                    <instr_i32shru as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32shru, components, verify)
                }
                Self::instr_i64rotl => {
                    <instr_i64rotl as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64rotl, components, verify)
                }
                Self::instr_i32rotl => {
                    <instr_i32rotl as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32rotl, components, verify)
                }
                Self::instr_i64rotr => {
                    <instr_i64rotr as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i64rotr, components, verify)
                }
                Self::instr_i32rotr => {
                    <instr_i32rotr as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&instr_i32rotr, components, verify)
                }
                Self::print_schedule => {
                    <print_schedule as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::instance(&print_schedule, components, verify)
                }
            }
        }
    }
    #[cfg(any(feature = "runtime-benchmarks", test))]
    impl<T: Config> ::frame_benchmarking::Benchmarking for Pallet<T>
    where
        T: frame_system::Config,
        T::AccountId: Origin,
    {
        fn benchmarks(
            extra: bool,
        ) -> ::frame_benchmarking::Vec<::frame_benchmarking::BenchmarkMetadata> {
            let mut all_names = <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    "check_all".as_ref(),
                    "check_lazy_pages_all".as_ref(),
                    "check_syscalls_integrity".as_ref(),
                    "check_lazy_pages_charging".as_ref(),
                    "check_lazy_pages_charging_special".as_ref(),
                    "check_lazy_pages_gas_exceed".as_ref(),
                    "db_write_per_kb".as_ref(),
                    "db_read_per_kb".as_ref(),
                    "instantiate_module_per_kb".as_ref(),
                    "claim_value".as_ref(),
                    "pay_program_rent".as_ref(),
                    "resume_session_init".as_ref(),
                    "resume_session_push".as_ref(),
                    "resume_session_commit".as_ref(),
                    "upload_code".as_ref(),
                    "create_program".as_ref(),
                    "upload_program".as_ref(),
                    "send_message".as_ref(),
                    "send_reply".as_ref(),
                    "initial_allocation".as_ref(),
                    "alloc_in_handle".as_ref(),
                    "reinstrument_per_kb".as_ref(),
                    "alloc".as_ref(),
                    "alloc_per_page".as_ref(),
                    "free".as_ref(),
                    "gr_reserve_gas".as_ref(),
                    "gr_unreserve_gas".as_ref(),
                    "gr_system_reserve_gas".as_ref(),
                    "gr_message_id".as_ref(),
                    "gr_origin".as_ref(),
                    "gr_program_id".as_ref(),
                    "gr_source".as_ref(),
                    "gr_value".as_ref(),
                    "gr_value_available".as_ref(),
                    "gr_gas_available".as_ref(),
                    "gr_size".as_ref(),
                    "gr_read".as_ref(),
                    "gr_read_per_kb".as_ref(),
                    "gr_block_height".as_ref(),
                    "gr_block_timestamp".as_ref(),
                    "gr_random".as_ref(),
                    "gr_reply_deposit".as_ref(),
                    "gr_send".as_ref(),
                    "gr_send_per_kb".as_ref(),
                    "gr_send_wgas".as_ref(),
                    "gr_send_wgas_per_kb".as_ref(),
                    "gr_send_input".as_ref(),
                    "gr_send_input_wgas".as_ref(),
                    "gr_send_init".as_ref(),
                    "gr_send_push".as_ref(),
                    "gr_send_push_per_kb".as_ref(),
                    "gr_send_commit".as_ref(),
                    "gr_send_commit_wgas".as_ref(),
                    "gr_reservation_send".as_ref(),
                    "gr_reservation_send_per_kb".as_ref(),
                    "gr_reservation_send_commit".as_ref(),
                    "gr_reply".as_ref(),
                    "gr_reply_per_kb".as_ref(),
                    "gr_reply_wgas".as_ref(),
                    "gr_reply_wgas_per_kb".as_ref(),
                    "gr_reply_commit".as_ref(),
                    "gr_reply_commit_wgas".as_ref(),
                    "gr_reply_push".as_ref(),
                    "gr_reply_push_per_kb".as_ref(),
                    "gr_reply_input".as_ref(),
                    "gr_reply_input_wgas".as_ref(),
                    "gr_reservation_reply".as_ref(),
                    "gr_reservation_reply_per_kb".as_ref(),
                    "gr_reservation_reply_commit".as_ref(),
                    "gr_reservation_reply_commit_per_kb".as_ref(),
                    "gr_reply_to".as_ref(),
                    "gr_signal_from".as_ref(),
                    "gr_reply_push_input".as_ref(),
                    "gr_reply_push_input_per_kb".as_ref(),
                    "gr_send_push_input".as_ref(),
                    "gr_send_push_input_per_kb".as_ref(),
                    "gr_debug".as_ref(),
                    "gr_debug_per_kb".as_ref(),
                    "gr_error".as_ref(),
                    "gr_status_code".as_ref(),
                    "gr_exit".as_ref(),
                    "gr_leave".as_ref(),
                    "gr_wait".as_ref(),
                    "gr_wait_for".as_ref(),
                    "gr_wait_up_to".as_ref(),
                    "gr_wake".as_ref(),
                    "gr_create_program".as_ref(),
                    "gr_create_program_per_kb".as_ref(),
                    "gr_create_program_wgas".as_ref(),
                    "gr_create_program_wgas_per_kb".as_ref(),
                    "gr_pay_program_rent".as_ref(),
                    "lazy_pages_signal_read".as_ref(),
                    "lazy_pages_signal_write".as_ref(),
                    "lazy_pages_signal_write_after_read".as_ref(),
                    "lazy_pages_load_page_storage_data".as_ref(),
                    "lazy_pages_host_func_read".as_ref(),
                    "lazy_pages_host_func_write".as_ref(),
                    "lazy_pages_host_func_write_after_read".as_ref(),
                    "mem_grow".as_ref(),
                    "instr_i64load".as_ref(),
                    "instr_i32load".as_ref(),
                    "instr_i64store".as_ref(),
                    "instr_i32store".as_ref(),
                    "instr_select".as_ref(),
                    "instr_if".as_ref(),
                    "instr_br".as_ref(),
                    "instr_br_if".as_ref(),
                    "instr_br_table".as_ref(),
                    "instr_br_table_per_entry".as_ref(),
                    "instr_call_const".as_ref(),
                    "instr_call".as_ref(),
                    "instr_call_indirect".as_ref(),
                    "instr_call_indirect_per_param".as_ref(),
                    "instr_call_per_local".as_ref(),
                    "instr_local_get".as_ref(),
                    "instr_local_set".as_ref(),
                    "instr_local_tee".as_ref(),
                    "instr_global_get".as_ref(),
                    "instr_global_set".as_ref(),
                    "instr_memory_current".as_ref(),
                    "instr_i64clz".as_ref(),
                    "instr_i32clz".as_ref(),
                    "instr_i64ctz".as_ref(),
                    "instr_i32ctz".as_ref(),
                    "instr_i64popcnt".as_ref(),
                    "instr_i32popcnt".as_ref(),
                    "instr_i64eqz".as_ref(),
                    "instr_i32eqz".as_ref(),
                    "instr_i32extend8s".as_ref(),
                    "instr_i32extend16s".as_ref(),
                    "instr_i64extend8s".as_ref(),
                    "instr_i64extend16s".as_ref(),
                    "instr_i64extend32s".as_ref(),
                    "instr_i64extendsi32".as_ref(),
                    "instr_i64extendui32".as_ref(),
                    "instr_i32wrapi64".as_ref(),
                    "instr_i64eq".as_ref(),
                    "instr_i32eq".as_ref(),
                    "instr_i64ne".as_ref(),
                    "instr_i32ne".as_ref(),
                    "instr_i64lts".as_ref(),
                    "instr_i32lts".as_ref(),
                    "instr_i64ltu".as_ref(),
                    "instr_i32ltu".as_ref(),
                    "instr_i64gts".as_ref(),
                    "instr_i32gts".as_ref(),
                    "instr_i64gtu".as_ref(),
                    "instr_i32gtu".as_ref(),
                    "instr_i64les".as_ref(),
                    "instr_i32les".as_ref(),
                    "instr_i64leu".as_ref(),
                    "instr_i32leu".as_ref(),
                    "instr_i64ges".as_ref(),
                    "instr_i32ges".as_ref(),
                    "instr_i64geu".as_ref(),
                    "instr_i32geu".as_ref(),
                    "instr_i64add".as_ref(),
                    "instr_i32add".as_ref(),
                    "instr_i64sub".as_ref(),
                    "instr_i32sub".as_ref(),
                    "instr_i64mul".as_ref(),
                    "instr_i32mul".as_ref(),
                    "instr_i64divs".as_ref(),
                    "instr_i32divs".as_ref(),
                    "instr_i64divu".as_ref(),
                    "instr_i32divu".as_ref(),
                    "instr_i64rems".as_ref(),
                    "instr_i32rems".as_ref(),
                    "instr_i64remu".as_ref(),
                    "instr_i32remu".as_ref(),
                    "instr_i64and".as_ref(),
                    "instr_i32and".as_ref(),
                    "instr_i64or".as_ref(),
                    "instr_i32or".as_ref(),
                    "instr_i64xor".as_ref(),
                    "instr_i32xor".as_ref(),
                    "instr_i64shl".as_ref(),
                    "instr_i32shl".as_ref(),
                    "instr_i64shrs".as_ref(),
                    "instr_i32shrs".as_ref(),
                    "instr_i64shru".as_ref(),
                    "instr_i32shru".as_ref(),
                    "instr_i64rotl".as_ref(),
                    "instr_i32rotl".as_ref(),
                    "instr_i64rotr".as_ref(),
                    "instr_i32rotr".as_ref(),
                    "print_schedule".as_ref(),
                ]),
            );
            if !extra {
                let extra = [
                    "check_all".as_ref(),
                    "check_lazy_pages_all".as_ref(),
                    "check_syscalls_integrity".as_ref(),
                    "check_lazy_pages_charging".as_ref(),
                    "check_lazy_pages_charging_special".as_ref(),
                    "check_lazy_pages_gas_exceed".as_ref(),
                    "print_schedule".as_ref(),
                ];
                all_names.retain(|x| !extra.contains(x));
            }
            let pov_modes: ::frame_benchmarking::Vec<
                (
                    ::frame_benchmarking::Vec<u8>,
                    ::frame_benchmarking::Vec<
                        (::frame_benchmarking::Vec<u8>, ::frame_benchmarking::Vec<u8>),
                    >,
                ),
            > = ::alloc::vec::Vec::new();
            all_names
                .into_iter()
                .map(|benchmark| {
                    let selected_benchmark = match benchmark {
                        "check_all" => SelectedBenchmark::check_all,
                        "check_lazy_pages_all" => SelectedBenchmark::check_lazy_pages_all,
                        "check_syscalls_integrity" => {
                            SelectedBenchmark::check_syscalls_integrity
                        }
                        "check_lazy_pages_charging" => {
                            SelectedBenchmark::check_lazy_pages_charging
                        }
                        "check_lazy_pages_charging_special" => {
                            SelectedBenchmark::check_lazy_pages_charging_special
                        }
                        "check_lazy_pages_gas_exceed" => {
                            SelectedBenchmark::check_lazy_pages_gas_exceed
                        }
                        "db_write_per_kb" => SelectedBenchmark::db_write_per_kb,
                        "db_read_per_kb" => SelectedBenchmark::db_read_per_kb,
                        "instantiate_module_per_kb" => {
                            SelectedBenchmark::instantiate_module_per_kb
                        }
                        "claim_value" => SelectedBenchmark::claim_value,
                        "pay_program_rent" => SelectedBenchmark::pay_program_rent,
                        "resume_session_init" => SelectedBenchmark::resume_session_init,
                        "resume_session_push" => SelectedBenchmark::resume_session_push,
                        "resume_session_commit" => {
                            SelectedBenchmark::resume_session_commit
                        }
                        "upload_code" => SelectedBenchmark::upload_code,
                        "create_program" => SelectedBenchmark::create_program,
                        "upload_program" => SelectedBenchmark::upload_program,
                        "send_message" => SelectedBenchmark::send_message,
                        "send_reply" => SelectedBenchmark::send_reply,
                        "initial_allocation" => SelectedBenchmark::initial_allocation,
                        "alloc_in_handle" => SelectedBenchmark::alloc_in_handle,
                        "reinstrument_per_kb" => SelectedBenchmark::reinstrument_per_kb,
                        "alloc" => SelectedBenchmark::alloc,
                        "alloc_per_page" => SelectedBenchmark::alloc_per_page,
                        "free" => SelectedBenchmark::free,
                        "gr_reserve_gas" => SelectedBenchmark::gr_reserve_gas,
                        "gr_unreserve_gas" => SelectedBenchmark::gr_unreserve_gas,
                        "gr_system_reserve_gas" => {
                            SelectedBenchmark::gr_system_reserve_gas
                        }
                        "gr_message_id" => SelectedBenchmark::gr_message_id,
                        "gr_origin" => SelectedBenchmark::gr_origin,
                        "gr_program_id" => SelectedBenchmark::gr_program_id,
                        "gr_source" => SelectedBenchmark::gr_source,
                        "gr_value" => SelectedBenchmark::gr_value,
                        "gr_value_available" => SelectedBenchmark::gr_value_available,
                        "gr_gas_available" => SelectedBenchmark::gr_gas_available,
                        "gr_size" => SelectedBenchmark::gr_size,
                        "gr_read" => SelectedBenchmark::gr_read,
                        "gr_read_per_kb" => SelectedBenchmark::gr_read_per_kb,
                        "gr_block_height" => SelectedBenchmark::gr_block_height,
                        "gr_block_timestamp" => SelectedBenchmark::gr_block_timestamp,
                        "gr_random" => SelectedBenchmark::gr_random,
                        "gr_reply_deposit" => SelectedBenchmark::gr_reply_deposit,
                        "gr_send" => SelectedBenchmark::gr_send,
                        "gr_send_per_kb" => SelectedBenchmark::gr_send_per_kb,
                        "gr_send_wgas" => SelectedBenchmark::gr_send_wgas,
                        "gr_send_wgas_per_kb" => SelectedBenchmark::gr_send_wgas_per_kb,
                        "gr_send_input" => SelectedBenchmark::gr_send_input,
                        "gr_send_input_wgas" => SelectedBenchmark::gr_send_input_wgas,
                        "gr_send_init" => SelectedBenchmark::gr_send_init,
                        "gr_send_push" => SelectedBenchmark::gr_send_push,
                        "gr_send_push_per_kb" => SelectedBenchmark::gr_send_push_per_kb,
                        "gr_send_commit" => SelectedBenchmark::gr_send_commit,
                        "gr_send_commit_wgas" => SelectedBenchmark::gr_send_commit_wgas,
                        "gr_reservation_send" => SelectedBenchmark::gr_reservation_send,
                        "gr_reservation_send_per_kb" => {
                            SelectedBenchmark::gr_reservation_send_per_kb
                        }
                        "gr_reservation_send_commit" => {
                            SelectedBenchmark::gr_reservation_send_commit
                        }
                        "gr_reply" => SelectedBenchmark::gr_reply,
                        "gr_reply_per_kb" => SelectedBenchmark::gr_reply_per_kb,
                        "gr_reply_wgas" => SelectedBenchmark::gr_reply_wgas,
                        "gr_reply_wgas_per_kb" => SelectedBenchmark::gr_reply_wgas_per_kb,
                        "gr_reply_commit" => SelectedBenchmark::gr_reply_commit,
                        "gr_reply_commit_wgas" => SelectedBenchmark::gr_reply_commit_wgas,
                        "gr_reply_push" => SelectedBenchmark::gr_reply_push,
                        "gr_reply_push_per_kb" => SelectedBenchmark::gr_reply_push_per_kb,
                        "gr_reply_input" => SelectedBenchmark::gr_reply_input,
                        "gr_reply_input_wgas" => SelectedBenchmark::gr_reply_input_wgas,
                        "gr_reservation_reply" => SelectedBenchmark::gr_reservation_reply,
                        "gr_reservation_reply_per_kb" => {
                            SelectedBenchmark::gr_reservation_reply_per_kb
                        }
                        "gr_reservation_reply_commit" => {
                            SelectedBenchmark::gr_reservation_reply_commit
                        }
                        "gr_reservation_reply_commit_per_kb" => {
                            SelectedBenchmark::gr_reservation_reply_commit_per_kb
                        }
                        "gr_reply_to" => SelectedBenchmark::gr_reply_to,
                        "gr_signal_from" => SelectedBenchmark::gr_signal_from,
                        "gr_reply_push_input" => SelectedBenchmark::gr_reply_push_input,
                        "gr_reply_push_input_per_kb" => {
                            SelectedBenchmark::gr_reply_push_input_per_kb
                        }
                        "gr_send_push_input" => SelectedBenchmark::gr_send_push_input,
                        "gr_send_push_input_per_kb" => {
                            SelectedBenchmark::gr_send_push_input_per_kb
                        }
                        "gr_debug" => SelectedBenchmark::gr_debug,
                        "gr_debug_per_kb" => SelectedBenchmark::gr_debug_per_kb,
                        "gr_error" => SelectedBenchmark::gr_error,
                        "gr_status_code" => SelectedBenchmark::gr_status_code,
                        "gr_exit" => SelectedBenchmark::gr_exit,
                        "gr_leave" => SelectedBenchmark::gr_leave,
                        "gr_wait" => SelectedBenchmark::gr_wait,
                        "gr_wait_for" => SelectedBenchmark::gr_wait_for,
                        "gr_wait_up_to" => SelectedBenchmark::gr_wait_up_to,
                        "gr_wake" => SelectedBenchmark::gr_wake,
                        "gr_create_program" => SelectedBenchmark::gr_create_program,
                        "gr_create_program_per_kb" => {
                            SelectedBenchmark::gr_create_program_per_kb
                        }
                        "gr_create_program_wgas" => {
                            SelectedBenchmark::gr_create_program_wgas
                        }
                        "gr_create_program_wgas_per_kb" => {
                            SelectedBenchmark::gr_create_program_wgas_per_kb
                        }
                        "gr_pay_program_rent" => SelectedBenchmark::gr_pay_program_rent,
                        "lazy_pages_signal_read" => {
                            SelectedBenchmark::lazy_pages_signal_read
                        }
                        "lazy_pages_signal_write" => {
                            SelectedBenchmark::lazy_pages_signal_write
                        }
                        "lazy_pages_signal_write_after_read" => {
                            SelectedBenchmark::lazy_pages_signal_write_after_read
                        }
                        "lazy_pages_load_page_storage_data" => {
                            SelectedBenchmark::lazy_pages_load_page_storage_data
                        }
                        "lazy_pages_host_func_read" => {
                            SelectedBenchmark::lazy_pages_host_func_read
                        }
                        "lazy_pages_host_func_write" => {
                            SelectedBenchmark::lazy_pages_host_func_write
                        }
                        "lazy_pages_host_func_write_after_read" => {
                            SelectedBenchmark::lazy_pages_host_func_write_after_read
                        }
                        "mem_grow" => SelectedBenchmark::mem_grow,
                        "instr_i64load" => SelectedBenchmark::instr_i64load,
                        "instr_i32load" => SelectedBenchmark::instr_i32load,
                        "instr_i64store" => SelectedBenchmark::instr_i64store,
                        "instr_i32store" => SelectedBenchmark::instr_i32store,
                        "instr_select" => SelectedBenchmark::instr_select,
                        "instr_if" => SelectedBenchmark::instr_if,
                        "instr_br" => SelectedBenchmark::instr_br,
                        "instr_br_if" => SelectedBenchmark::instr_br_if,
                        "instr_br_table" => SelectedBenchmark::instr_br_table,
                        "instr_br_table_per_entry" => {
                            SelectedBenchmark::instr_br_table_per_entry
                        }
                        "instr_call_const" => SelectedBenchmark::instr_call_const,
                        "instr_call" => SelectedBenchmark::instr_call,
                        "instr_call_indirect" => SelectedBenchmark::instr_call_indirect,
                        "instr_call_indirect_per_param" => {
                            SelectedBenchmark::instr_call_indirect_per_param
                        }
                        "instr_call_per_local" => SelectedBenchmark::instr_call_per_local,
                        "instr_local_get" => SelectedBenchmark::instr_local_get,
                        "instr_local_set" => SelectedBenchmark::instr_local_set,
                        "instr_local_tee" => SelectedBenchmark::instr_local_tee,
                        "instr_global_get" => SelectedBenchmark::instr_global_get,
                        "instr_global_set" => SelectedBenchmark::instr_global_set,
                        "instr_memory_current" => SelectedBenchmark::instr_memory_current,
                        "instr_i64clz" => SelectedBenchmark::instr_i64clz,
                        "instr_i32clz" => SelectedBenchmark::instr_i32clz,
                        "instr_i64ctz" => SelectedBenchmark::instr_i64ctz,
                        "instr_i32ctz" => SelectedBenchmark::instr_i32ctz,
                        "instr_i64popcnt" => SelectedBenchmark::instr_i64popcnt,
                        "instr_i32popcnt" => SelectedBenchmark::instr_i32popcnt,
                        "instr_i64eqz" => SelectedBenchmark::instr_i64eqz,
                        "instr_i32eqz" => SelectedBenchmark::instr_i32eqz,
                        "instr_i32extend8s" => SelectedBenchmark::instr_i32extend8s,
                        "instr_i32extend16s" => SelectedBenchmark::instr_i32extend16s,
                        "instr_i64extend8s" => SelectedBenchmark::instr_i64extend8s,
                        "instr_i64extend16s" => SelectedBenchmark::instr_i64extend16s,
                        "instr_i64extend32s" => SelectedBenchmark::instr_i64extend32s,
                        "instr_i64extendsi32" => SelectedBenchmark::instr_i64extendsi32,
                        "instr_i64extendui32" => SelectedBenchmark::instr_i64extendui32,
                        "instr_i32wrapi64" => SelectedBenchmark::instr_i32wrapi64,
                        "instr_i64eq" => SelectedBenchmark::instr_i64eq,
                        "instr_i32eq" => SelectedBenchmark::instr_i32eq,
                        "instr_i64ne" => SelectedBenchmark::instr_i64ne,
                        "instr_i32ne" => SelectedBenchmark::instr_i32ne,
                        "instr_i64lts" => SelectedBenchmark::instr_i64lts,
                        "instr_i32lts" => SelectedBenchmark::instr_i32lts,
                        "instr_i64ltu" => SelectedBenchmark::instr_i64ltu,
                        "instr_i32ltu" => SelectedBenchmark::instr_i32ltu,
                        "instr_i64gts" => SelectedBenchmark::instr_i64gts,
                        "instr_i32gts" => SelectedBenchmark::instr_i32gts,
                        "instr_i64gtu" => SelectedBenchmark::instr_i64gtu,
                        "instr_i32gtu" => SelectedBenchmark::instr_i32gtu,
                        "instr_i64les" => SelectedBenchmark::instr_i64les,
                        "instr_i32les" => SelectedBenchmark::instr_i32les,
                        "instr_i64leu" => SelectedBenchmark::instr_i64leu,
                        "instr_i32leu" => SelectedBenchmark::instr_i32leu,
                        "instr_i64ges" => SelectedBenchmark::instr_i64ges,
                        "instr_i32ges" => SelectedBenchmark::instr_i32ges,
                        "instr_i64geu" => SelectedBenchmark::instr_i64geu,
                        "instr_i32geu" => SelectedBenchmark::instr_i32geu,
                        "instr_i64add" => SelectedBenchmark::instr_i64add,
                        "instr_i32add" => SelectedBenchmark::instr_i32add,
                        "instr_i64sub" => SelectedBenchmark::instr_i64sub,
                        "instr_i32sub" => SelectedBenchmark::instr_i32sub,
                        "instr_i64mul" => SelectedBenchmark::instr_i64mul,
                        "instr_i32mul" => SelectedBenchmark::instr_i32mul,
                        "instr_i64divs" => SelectedBenchmark::instr_i64divs,
                        "instr_i32divs" => SelectedBenchmark::instr_i32divs,
                        "instr_i64divu" => SelectedBenchmark::instr_i64divu,
                        "instr_i32divu" => SelectedBenchmark::instr_i32divu,
                        "instr_i64rems" => SelectedBenchmark::instr_i64rems,
                        "instr_i32rems" => SelectedBenchmark::instr_i32rems,
                        "instr_i64remu" => SelectedBenchmark::instr_i64remu,
                        "instr_i32remu" => SelectedBenchmark::instr_i32remu,
                        "instr_i64and" => SelectedBenchmark::instr_i64and,
                        "instr_i32and" => SelectedBenchmark::instr_i32and,
                        "instr_i64or" => SelectedBenchmark::instr_i64or,
                        "instr_i32or" => SelectedBenchmark::instr_i32or,
                        "instr_i64xor" => SelectedBenchmark::instr_i64xor,
                        "instr_i32xor" => SelectedBenchmark::instr_i32xor,
                        "instr_i64shl" => SelectedBenchmark::instr_i64shl,
                        "instr_i32shl" => SelectedBenchmark::instr_i32shl,
                        "instr_i64shrs" => SelectedBenchmark::instr_i64shrs,
                        "instr_i32shrs" => SelectedBenchmark::instr_i32shrs,
                        "instr_i64shru" => SelectedBenchmark::instr_i64shru,
                        "instr_i32shru" => SelectedBenchmark::instr_i32shru,
                        "instr_i64rotl" => SelectedBenchmark::instr_i64rotl,
                        "instr_i32rotl" => SelectedBenchmark::instr_i32rotl,
                        "instr_i64rotr" => SelectedBenchmark::instr_i64rotr,
                        "instr_i32rotr" => SelectedBenchmark::instr_i32rotr,
                        "print_schedule" => SelectedBenchmark::print_schedule,
                        _ => {
                            ::core::panicking::panic_fmt(
                                format_args!("all benchmarks should be selectable"),
                            )
                        }
                    };
                    let name = benchmark.as_bytes().to_vec();
                    let components = <SelectedBenchmark as ::frame_benchmarking::BenchmarkingSetup<
                        T,
                    >>::components(&selected_benchmark);
                    ::frame_benchmarking::BenchmarkMetadata {
                        name: name.clone(),
                        components,
                        pov_modes: pov_modes
                            .iter()
                            .find(|p| p.0 == name)
                            .map(|p| p.1.clone())
                            .unwrap_or_default(),
                    }
                })
                .collect::<::frame_benchmarking::Vec<_>>()
        }
        fn run_benchmark(
            extrinsic: &[u8],
            c: &[(::frame_benchmarking::BenchmarkParameter, u32)],
            whitelist: &[::frame_benchmarking::TrackedStorageKey],
            verify: bool,
            internal_repeats: u32,
        ) -> Result<
            ::frame_benchmarking::Vec<::frame_benchmarking::BenchmarkResult>,
            ::frame_benchmarking::BenchmarkError,
        > {
            let extrinsic = ::frame_benchmarking::str::from_utf8(extrinsic)
                .map_err(|_| "`extrinsic` is not a valid utf8 string!")?;
            let selected_benchmark = match extrinsic {
                "check_all" => SelectedBenchmark::check_all,
                "check_lazy_pages_all" => SelectedBenchmark::check_lazy_pages_all,
                "check_syscalls_integrity" => SelectedBenchmark::check_syscalls_integrity,
                "check_lazy_pages_charging" => {
                    SelectedBenchmark::check_lazy_pages_charging
                }
                "check_lazy_pages_charging_special" => {
                    SelectedBenchmark::check_lazy_pages_charging_special
                }
                "check_lazy_pages_gas_exceed" => {
                    SelectedBenchmark::check_lazy_pages_gas_exceed
                }
                "db_write_per_kb" => SelectedBenchmark::db_write_per_kb,
                "db_read_per_kb" => SelectedBenchmark::db_read_per_kb,
                "instantiate_module_per_kb" => {
                    SelectedBenchmark::instantiate_module_per_kb
                }
                "claim_value" => SelectedBenchmark::claim_value,
                "pay_program_rent" => SelectedBenchmark::pay_program_rent,
                "resume_session_init" => SelectedBenchmark::resume_session_init,
                "resume_session_push" => SelectedBenchmark::resume_session_push,
                "resume_session_commit" => SelectedBenchmark::resume_session_commit,
                "upload_code" => SelectedBenchmark::upload_code,
                "create_program" => SelectedBenchmark::create_program,
                "upload_program" => SelectedBenchmark::upload_program,
                "send_message" => SelectedBenchmark::send_message,
                "send_reply" => SelectedBenchmark::send_reply,
                "initial_allocation" => SelectedBenchmark::initial_allocation,
                "alloc_in_handle" => SelectedBenchmark::alloc_in_handle,
                "reinstrument_per_kb" => SelectedBenchmark::reinstrument_per_kb,
                "alloc" => SelectedBenchmark::alloc,
                "alloc_per_page" => SelectedBenchmark::alloc_per_page,
                "free" => SelectedBenchmark::free,
                "gr_reserve_gas" => SelectedBenchmark::gr_reserve_gas,
                "gr_unreserve_gas" => SelectedBenchmark::gr_unreserve_gas,
                "gr_system_reserve_gas" => SelectedBenchmark::gr_system_reserve_gas,
                "gr_message_id" => SelectedBenchmark::gr_message_id,
                "gr_origin" => SelectedBenchmark::gr_origin,
                "gr_program_id" => SelectedBenchmark::gr_program_id,
                "gr_source" => SelectedBenchmark::gr_source,
                "gr_value" => SelectedBenchmark::gr_value,
                "gr_value_available" => SelectedBenchmark::gr_value_available,
                "gr_gas_available" => SelectedBenchmark::gr_gas_available,
                "gr_size" => SelectedBenchmark::gr_size,
                "gr_read" => SelectedBenchmark::gr_read,
                "gr_read_per_kb" => SelectedBenchmark::gr_read_per_kb,
                "gr_block_height" => SelectedBenchmark::gr_block_height,
                "gr_block_timestamp" => SelectedBenchmark::gr_block_timestamp,
                "gr_random" => SelectedBenchmark::gr_random,
                "gr_reply_deposit" => SelectedBenchmark::gr_reply_deposit,
                "gr_send" => SelectedBenchmark::gr_send,
                "gr_send_per_kb" => SelectedBenchmark::gr_send_per_kb,
                "gr_send_wgas" => SelectedBenchmark::gr_send_wgas,
                "gr_send_wgas_per_kb" => SelectedBenchmark::gr_send_wgas_per_kb,
                "gr_send_input" => SelectedBenchmark::gr_send_input,
                "gr_send_input_wgas" => SelectedBenchmark::gr_send_input_wgas,
                "gr_send_init" => SelectedBenchmark::gr_send_init,
                "gr_send_push" => SelectedBenchmark::gr_send_push,
                "gr_send_push_per_kb" => SelectedBenchmark::gr_send_push_per_kb,
                "gr_send_commit" => SelectedBenchmark::gr_send_commit,
                "gr_send_commit_wgas" => SelectedBenchmark::gr_send_commit_wgas,
                "gr_reservation_send" => SelectedBenchmark::gr_reservation_send,
                "gr_reservation_send_per_kb" => {
                    SelectedBenchmark::gr_reservation_send_per_kb
                }
                "gr_reservation_send_commit" => {
                    SelectedBenchmark::gr_reservation_send_commit
                }
                "gr_reply" => SelectedBenchmark::gr_reply,
                "gr_reply_per_kb" => SelectedBenchmark::gr_reply_per_kb,
                "gr_reply_wgas" => SelectedBenchmark::gr_reply_wgas,
                "gr_reply_wgas_per_kb" => SelectedBenchmark::gr_reply_wgas_per_kb,
                "gr_reply_commit" => SelectedBenchmark::gr_reply_commit,
                "gr_reply_commit_wgas" => SelectedBenchmark::gr_reply_commit_wgas,
                "gr_reply_push" => SelectedBenchmark::gr_reply_push,
                "gr_reply_push_per_kb" => SelectedBenchmark::gr_reply_push_per_kb,
                "gr_reply_input" => SelectedBenchmark::gr_reply_input,
                "gr_reply_input_wgas" => SelectedBenchmark::gr_reply_input_wgas,
                "gr_reservation_reply" => SelectedBenchmark::gr_reservation_reply,
                "gr_reservation_reply_per_kb" => {
                    SelectedBenchmark::gr_reservation_reply_per_kb
                }
                "gr_reservation_reply_commit" => {
                    SelectedBenchmark::gr_reservation_reply_commit
                }
                "gr_reservation_reply_commit_per_kb" => {
                    SelectedBenchmark::gr_reservation_reply_commit_per_kb
                }
                "gr_reply_to" => SelectedBenchmark::gr_reply_to,
                "gr_signal_from" => SelectedBenchmark::gr_signal_from,
                "gr_reply_push_input" => SelectedBenchmark::gr_reply_push_input,
                "gr_reply_push_input_per_kb" => {
                    SelectedBenchmark::gr_reply_push_input_per_kb
                }
                "gr_send_push_input" => SelectedBenchmark::gr_send_push_input,
                "gr_send_push_input_per_kb" => {
                    SelectedBenchmark::gr_send_push_input_per_kb
                }
                "gr_debug" => SelectedBenchmark::gr_debug,
                "gr_debug_per_kb" => SelectedBenchmark::gr_debug_per_kb,
                "gr_error" => SelectedBenchmark::gr_error,
                "gr_status_code" => SelectedBenchmark::gr_status_code,
                "gr_exit" => SelectedBenchmark::gr_exit,
                "gr_leave" => SelectedBenchmark::gr_leave,
                "gr_wait" => SelectedBenchmark::gr_wait,
                "gr_wait_for" => SelectedBenchmark::gr_wait_for,
                "gr_wait_up_to" => SelectedBenchmark::gr_wait_up_to,
                "gr_wake" => SelectedBenchmark::gr_wake,
                "gr_create_program" => SelectedBenchmark::gr_create_program,
                "gr_create_program_per_kb" => SelectedBenchmark::gr_create_program_per_kb,
                "gr_create_program_wgas" => SelectedBenchmark::gr_create_program_wgas,
                "gr_create_program_wgas_per_kb" => {
                    SelectedBenchmark::gr_create_program_wgas_per_kb
                }
                "gr_pay_program_rent" => SelectedBenchmark::gr_pay_program_rent,
                "lazy_pages_signal_read" => SelectedBenchmark::lazy_pages_signal_read,
                "lazy_pages_signal_write" => SelectedBenchmark::lazy_pages_signal_write,
                "lazy_pages_signal_write_after_read" => {
                    SelectedBenchmark::lazy_pages_signal_write_after_read
                }
                "lazy_pages_load_page_storage_data" => {
                    SelectedBenchmark::lazy_pages_load_page_storage_data
                }
                "lazy_pages_host_func_read" => {
                    SelectedBenchmark::lazy_pages_host_func_read
                }
                "lazy_pages_host_func_write" => {
                    SelectedBenchmark::lazy_pages_host_func_write
                }
                "lazy_pages_host_func_write_after_read" => {
                    SelectedBenchmark::lazy_pages_host_func_write_after_read
                }
                "mem_grow" => SelectedBenchmark::mem_grow,
                "instr_i64load" => SelectedBenchmark::instr_i64load,
                "instr_i32load" => SelectedBenchmark::instr_i32load,
                "instr_i64store" => SelectedBenchmark::instr_i64store,
                "instr_i32store" => SelectedBenchmark::instr_i32store,
                "instr_select" => SelectedBenchmark::instr_select,
                "instr_if" => SelectedBenchmark::instr_if,
                "instr_br" => SelectedBenchmark::instr_br,
                "instr_br_if" => SelectedBenchmark::instr_br_if,
                "instr_br_table" => SelectedBenchmark::instr_br_table,
                "instr_br_table_per_entry" => SelectedBenchmark::instr_br_table_per_entry,
                "instr_call_const" => SelectedBenchmark::instr_call_const,
                "instr_call" => SelectedBenchmark::instr_call,
                "instr_call_indirect" => SelectedBenchmark::instr_call_indirect,
                "instr_call_indirect_per_param" => {
                    SelectedBenchmark::instr_call_indirect_per_param
                }
                "instr_call_per_local" => SelectedBenchmark::instr_call_per_local,
                "instr_local_get" => SelectedBenchmark::instr_local_get,
                "instr_local_set" => SelectedBenchmark::instr_local_set,
                "instr_local_tee" => SelectedBenchmark::instr_local_tee,
                "instr_global_get" => SelectedBenchmark::instr_global_get,
                "instr_global_set" => SelectedBenchmark::instr_global_set,
                "instr_memory_current" => SelectedBenchmark::instr_memory_current,
                "instr_i64clz" => SelectedBenchmark::instr_i64clz,
                "instr_i32clz" => SelectedBenchmark::instr_i32clz,
                "instr_i64ctz" => SelectedBenchmark::instr_i64ctz,
                "instr_i32ctz" => SelectedBenchmark::instr_i32ctz,
                "instr_i64popcnt" => SelectedBenchmark::instr_i64popcnt,
                "instr_i32popcnt" => SelectedBenchmark::instr_i32popcnt,
                "instr_i64eqz" => SelectedBenchmark::instr_i64eqz,
                "instr_i32eqz" => SelectedBenchmark::instr_i32eqz,
                "instr_i32extend8s" => SelectedBenchmark::instr_i32extend8s,
                "instr_i32extend16s" => SelectedBenchmark::instr_i32extend16s,
                "instr_i64extend8s" => SelectedBenchmark::instr_i64extend8s,
                "instr_i64extend16s" => SelectedBenchmark::instr_i64extend16s,
                "instr_i64extend32s" => SelectedBenchmark::instr_i64extend32s,
                "instr_i64extendsi32" => SelectedBenchmark::instr_i64extendsi32,
                "instr_i64extendui32" => SelectedBenchmark::instr_i64extendui32,
                "instr_i32wrapi64" => SelectedBenchmark::instr_i32wrapi64,
                "instr_i64eq" => SelectedBenchmark::instr_i64eq,
                "instr_i32eq" => SelectedBenchmark::instr_i32eq,
                "instr_i64ne" => SelectedBenchmark::instr_i64ne,
                "instr_i32ne" => SelectedBenchmark::instr_i32ne,
                "instr_i64lts" => SelectedBenchmark::instr_i64lts,
                "instr_i32lts" => SelectedBenchmark::instr_i32lts,
                "instr_i64ltu" => SelectedBenchmark::instr_i64ltu,
                "instr_i32ltu" => SelectedBenchmark::instr_i32ltu,
                "instr_i64gts" => SelectedBenchmark::instr_i64gts,
                "instr_i32gts" => SelectedBenchmark::instr_i32gts,
                "instr_i64gtu" => SelectedBenchmark::instr_i64gtu,
                "instr_i32gtu" => SelectedBenchmark::instr_i32gtu,
                "instr_i64les" => SelectedBenchmark::instr_i64les,
                "instr_i32les" => SelectedBenchmark::instr_i32les,
                "instr_i64leu" => SelectedBenchmark::instr_i64leu,
                "instr_i32leu" => SelectedBenchmark::instr_i32leu,
                "instr_i64ges" => SelectedBenchmark::instr_i64ges,
                "instr_i32ges" => SelectedBenchmark::instr_i32ges,
                "instr_i64geu" => SelectedBenchmark::instr_i64geu,
                "instr_i32geu" => SelectedBenchmark::instr_i32geu,
                "instr_i64add" => SelectedBenchmark::instr_i64add,
                "instr_i32add" => SelectedBenchmark::instr_i32add,
                "instr_i64sub" => SelectedBenchmark::instr_i64sub,
                "instr_i32sub" => SelectedBenchmark::instr_i32sub,
                "instr_i64mul" => SelectedBenchmark::instr_i64mul,
                "instr_i32mul" => SelectedBenchmark::instr_i32mul,
                "instr_i64divs" => SelectedBenchmark::instr_i64divs,
                "instr_i32divs" => SelectedBenchmark::instr_i32divs,
                "instr_i64divu" => SelectedBenchmark::instr_i64divu,
                "instr_i32divu" => SelectedBenchmark::instr_i32divu,
                "instr_i64rems" => SelectedBenchmark::instr_i64rems,
                "instr_i32rems" => SelectedBenchmark::instr_i32rems,
                "instr_i64remu" => SelectedBenchmark::instr_i64remu,
                "instr_i32remu" => SelectedBenchmark::instr_i32remu,
                "instr_i64and" => SelectedBenchmark::instr_i64and,
                "instr_i32and" => SelectedBenchmark::instr_i32and,
                "instr_i64or" => SelectedBenchmark::instr_i64or,
                "instr_i32or" => SelectedBenchmark::instr_i32or,
                "instr_i64xor" => SelectedBenchmark::instr_i64xor,
                "instr_i32xor" => SelectedBenchmark::instr_i32xor,
                "instr_i64shl" => SelectedBenchmark::instr_i64shl,
                "instr_i32shl" => SelectedBenchmark::instr_i32shl,
                "instr_i64shrs" => SelectedBenchmark::instr_i64shrs,
                "instr_i32shrs" => SelectedBenchmark::instr_i32shrs,
                "instr_i64shru" => SelectedBenchmark::instr_i64shru,
                "instr_i32shru" => SelectedBenchmark::instr_i32shru,
                "instr_i64rotl" => SelectedBenchmark::instr_i64rotl,
                "instr_i32rotl" => SelectedBenchmark::instr_i32rotl,
                "instr_i64rotr" => SelectedBenchmark::instr_i64rotr,
                "instr_i32rotr" => SelectedBenchmark::instr_i32rotr,
                "print_schedule" => SelectedBenchmark::print_schedule,
                _ => return Err("Could not find extrinsic.".into()),
            };
            let mut whitelist = whitelist.to_vec();
            let whitelisted_caller_key = <frame_system::Account<
                T,
            > as ::frame_benchmarking::frame_support::storage::StorageMap<
                _,
                _,
            >>::hashed_key_for(
                ::frame_benchmarking::whitelisted_caller::<T::AccountId>(),
            );
            whitelist.push(whitelisted_caller_key.into());
            let transactional_layer_key = ::frame_benchmarking::TrackedStorageKey::new(
                ::frame_benchmarking::frame_support::storage::transactional::TRANSACTION_LEVEL_KEY
                    .into(),
            );
            whitelist.push(transactional_layer_key);
            let extrinsic_index = ::frame_benchmarking::TrackedStorageKey::new(
                ::frame_benchmarking::well_known_keys::EXTRINSIC_INDEX.into(),
            );
            whitelist.push(extrinsic_index);
            ::frame_benchmarking::benchmarking::set_whitelist(whitelist.clone());
            let mut results: ::frame_benchmarking::Vec<
                ::frame_benchmarking::BenchmarkResult,
            > = ::frame_benchmarking::Vec::new();
            for _ in 0..internal_repeats.max(1) {
                let _guard = ::sp_core::defer::DeferGuard(
                    Some(|| { ::frame_benchmarking::benchmarking::wipe_db() }),
                );
                let closure_to_benchmark = <SelectedBenchmark as ::frame_benchmarking::BenchmarkingSetup<
                    T,
                >>::instance(&selected_benchmark, c, verify)?;
                if ::frame_benchmarking::Zero::is_zero(
                    &frame_system::Pallet::<T>::block_number(),
                ) {
                    frame_system::Pallet::<T>::set_block_number(1u32.into());
                }
                ::frame_benchmarking::benchmarking::commit_db();
                for key in &whitelist {
                    ::frame_benchmarking::frame_support::storage::unhashed::get_raw(
                        &key.key,
                    );
                }
                ::frame_benchmarking::benchmarking::reset_read_write_count();
                {
                    let lvl = ::log::Level::Trace;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api_log(
                            format_args!(
                                "Start Benchmark: {0} ({1:?}) verify {2}", extrinsic, c,
                                verify
                            ),
                            lvl,
                            &(
                                "benchmark",
                                "pallet_gear::benchmarking",
                                "pallets/gear/src/benchmarking/mod.rs",
                                359u32,
                            ),
                            ::log::__private_api::Option::None,
                        );
                    }
                };
                let start_pov = ::frame_benchmarking::benchmarking::proof_size();
                let start_extrinsic = ::frame_benchmarking::benchmarking::current_time();
                closure_to_benchmark()?;
                let finish_extrinsic = ::frame_benchmarking::benchmarking::current_time();
                let end_pov = ::frame_benchmarking::benchmarking::proof_size();
                let elapsed_extrinsic = finish_extrinsic.saturating_sub(start_extrinsic);
                let diff_pov = match (start_pov, end_pov) {
                    (Some(start), Some(end)) => end.saturating_sub(start),
                    _ => Default::default(),
                };
                ::frame_benchmarking::benchmarking::commit_db();
                {
                    let lvl = ::log::Level::Trace;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api_log(
                            format_args!("End Benchmark: {0} ns", elapsed_extrinsic),
                            lvl,
                            &(
                                "benchmark",
                                "pallet_gear::benchmarking",
                                "pallets/gear/src/benchmarking/mod.rs",
                                359u32,
                            ),
                            ::log::__private_api::Option::None,
                        );
                    }
                };
                let read_write_count = ::frame_benchmarking::benchmarking::read_write_count();
                {
                    let lvl = ::log::Level::Trace;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api_log(
                            format_args!("Read/Write Count {0:?}", read_write_count),
                            lvl,
                            &(
                                "benchmark",
                                "pallet_gear::benchmarking",
                                "pallets/gear/src/benchmarking/mod.rs",
                                359u32,
                            ),
                            ::log::__private_api::Option::None,
                        );
                    }
                };
                {
                    let lvl = ::log::Level::Trace;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api_log(
                            format_args!(
                                "Proof sizes: before {0:?} after {1:?} diff {2}", &
                                start_pov, & end_pov, & diff_pov
                            ),
                            lvl,
                            &(
                                "benchmark",
                                "pallet_gear::benchmarking",
                                "pallets/gear/src/benchmarking/mod.rs",
                                359u32,
                            ),
                            ::log::__private_api::Option::None,
                        );
                    }
                };
                let start_storage_root = ::frame_benchmarking::benchmarking::current_time();
                ::frame_benchmarking::storage_root(
                    ::frame_benchmarking::StateVersion::V1,
                );
                let finish_storage_root = ::frame_benchmarking::benchmarking::current_time();
                let elapsed_storage_root = finish_storage_root - start_storage_root;
                let skip_meta = [];
                let read_and_written_keys = if skip_meta.contains(&extrinsic) {
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            (b"Skipped Metadata".to_vec(), 0, 0, false),
                        ]),
                    )
                } else {
                    ::frame_benchmarking::benchmarking::get_read_and_written_keys()
                };
                results
                    .push(::frame_benchmarking::BenchmarkResult {
                        components: c.to_vec(),
                        extrinsic_time: elapsed_extrinsic,
                        storage_root_time: elapsed_storage_root,
                        reads: read_write_count.0,
                        repeat_reads: read_write_count.1,
                        writes: read_write_count.2,
                        repeat_writes: read_write_count.3,
                        proof_size: diff_pov,
                        keys: read_and_written_keys,
                    });
            }
            return Ok(results);
        }
    }
}
