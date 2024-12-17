/// Configuration for `Code::try_new_mock_`.
/// By default all checks enabled.
pub struct TryNewCodeConfig {
    /// Instrumentation version
    pub version: u32,
    /// Stack height limit
    pub stack_height: Option<u32>,
    /// Limit of data section amount
    pub data_segments_amount_limit: Option<u32>,
    /// Limit on the number of tables.
    pub table_number_limit: Option<u32>,
    /// Export `STACK_HEIGHT_EXPORT_NAME` global
    pub export_stack_height: bool,
    /// Check exports (wasm contains init or handle exports)
    pub check_exports: bool,
    /// Check imports (check that all imports are valid syscalls with correct signature)
    pub check_imports: bool,
    /// Check and canonize stack end
    pub check_and_canonize_stack_end: bool,
    /// Check mutable global exports
    pub check_mut_global_exports: bool,
    /// Check start section (not allowed for programs)
    pub check_start_section: bool,
    /// Check data section
    pub check_data_section: bool,
    /// Check table section
    pub check_table_section: bool,
    /// Make wasmparser validation
    pub make_validation: bool,
}

impl TryNewCodeConfig {
    /// New default config without exports checks.
    pub fn with_no_exports_check() -> Self {
        Self {
            check_exports: false,
            ..Default::default()
        }
    }
}

impl Default for TryNewCodeConfig {
    fn default() -> Self {
        Self {
            version: 1,
            stack_height: None,
            data_segments_amount_limit: None,
            table_number_limit: None,
            export_stack_height: false,
            check_exports: true,
            check_imports: true,
            check_and_canonize_stack_end: true,
            check_mut_global_exports: true,
            check_start_section: true,
            check_data_section: true,
            check_table_section: true,
            make_validation: true,
        }
    }
}
