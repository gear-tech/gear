use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

use crate::{
    message::DispatchKind,
    pages::{WasmPage, WasmPagesAmount},
};

use alloc::collections::BTreeSet;

/// Status of the instrumentation.
#[derive(Clone, Copy, Debug, Decode, Encode, TypeInfo, PartialEq, Eq)]
pub enum InstrumentationStatus {
    /// Code is instrumented.
    Instrumented(u32),
    /// Failed to instrument code.
    InstrumentationFailed(u32),
}

/// Metadata for the code.
#[derive(Clone, Debug, Decode, Encode, TypeInfo, PartialEq, Eq)]
pub struct CodeMetadata {
    /// Original code length.
    original_code_len: u32,
    /// Instrumented code length.
    instrumented_code_len: u32,
    /// Exports of the wasm module.
    exports: BTreeSet<DispatchKind>,
    // Static pages count from memory import.
    static_pages: WasmPagesAmount,
    /// Stack end page.
    stack_end: Option<WasmPage>,
    /// Instrumentation status, contains version of the instructions.
    instrumentation_status: InstrumentationStatus,
}

impl CodeMetadata {
    /// Creates a new instance of the code metadata.
    pub fn new(
        original_code_len: u32,
        instrumented_code_len: u32,
        exports: BTreeSet<DispatchKind>,
        static_pages: WasmPagesAmount,
        stack_end: Option<WasmPage>,
        instrumentation_status: InstrumentationStatus,
    ) -> Self {
        Self {
            original_code_len,
            instrumented_code_len,
            exports,
            static_pages,
            stack_end,
            instrumentation_status,
        }
    }

    /// Converts the metadata into the failed instrumentation state.
    pub fn into_failed_instrumentation(self, instruction_weights_version: u32) -> Self {
        Self {
            instrumentation_status: InstrumentationStatus::InstrumentationFailed(
                instruction_weights_version,
            ),
            ..self
        }
    }

    /// Returns the original code length.
    pub fn original_code_len(&self) -> u32 {
        self.original_code_len
    }

    /// Returns the instrumented code length.
    pub fn instrumented_code_len(&self) -> u32 {
        self.instrumented_code_len
    }

    /// Returns the code exports.
    pub fn exports(&self) -> &BTreeSet<DispatchKind> {
        &self.exports
    }

    /// Returns the static pages count from memory import.
    pub fn static_pages(&self) -> WasmPagesAmount {
        self.static_pages
    }

    /// Returns the stack end page.
    pub fn stack_end(&self) -> Option<WasmPage> {
        self.stack_end
    }

    /// Returns the instrumentation status.
    pub fn instrumentation_status(&self) -> InstrumentationStatus {
        self.instrumentation_status
    }

    /// Returns the version of the instructions.
    pub fn instruction_weights_version(&self) -> u32 {
        match self.instrumentation_status {
            InstrumentationStatus::Instrumented(version) => version,
            InstrumentationStatus::InstrumentationFailed(version) => version,
        }
    }
}
