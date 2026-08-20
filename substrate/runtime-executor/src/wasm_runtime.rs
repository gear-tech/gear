// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Traits and accessor functions for calling into the Substrate Wasm runtime.
//!
//! The primary means of accessing the runtimes is through a cache which saves the reusable
//! components of the runtime that are expensive to initialize.

use crate::error::{Error, WasmError};

use codec::Decode;
use parking_lot::Mutex;
use sc_executor_common::{
    runtime_blob::RuntimeBlob,
    wasm_runtime::{HeapAllocStrategy, WasmInstance, WasmModule},
};
use schnellru::{ByLength, LruMap};
use sp_core::traits::{Externalities, FetchRuntimeCode, RuntimeCode};
use sp_version::RuntimeVersion;
use sp_wasm_interface::HostFunctions;

use std::{
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Specification of different methods of executing the runtime Wasm code.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum WasmExecutionMethod {
    /// Uses the Wasmtime compiled runtime.
    Compiled {
        /// The instantiation strategy to use.
        instantiation_strategy: sc_executor_wasmtime::InstantiationStrategy,
    },
}

impl Default for WasmExecutionMethod {
    fn default() -> Self {
        Self::Compiled {
            instantiation_strategy: sc_executor_wasmtime::InstantiationStrategy::PoolingCopyOnWrite,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct VersionedRuntimeId {
    /// Runtime code hash.
    code_hash: Vec<u8>,
    /// Wasm runtime type.
    wasm_method: WasmExecutionMethod,
    /// The heap allocation strategy this runtime was created with.
    heap_alloc_strategy: HeapAllocStrategy,
}

/// A Wasm runtime object along with its cached runtime version.
struct VersionedRuntime {
    /// Shared runtime that can spawn instances.
    module: Box<dyn WasmModule>,
    /// Runtime version according to `Core_version` if any.
    version: Option<RuntimeVersion>,

    // TODO: Remove this once the legacy instance reuse instantiation strategy
    //       for `wasmtime` is gone, as this only makes sense with that particular strategy.
    /// Cached instance pool.
    instances: Vec<Mutex<Option<Box<dyn WasmInstance>>>>,
}

impl VersionedRuntime {
    /// Run the given closure `f` with an instance of this runtime.
    fn with_instance<R, F>(&self, ext: &mut dyn Externalities, f: F) -> Result<R, Error>
    where
        F: FnOnce(
            &dyn WasmModule,
            &mut dyn WasmInstance,
            Option<&RuntimeVersion>,
            &mut dyn Externalities,
        ) -> Result<R, Error>,
    {
        // Find a free instance
        let instance = self
            .instances
            .iter()
            .enumerate()
            .find_map(|(index, i)| i.try_lock().map(|i| (index, i)));

        match instance {
            Some((index, mut locked)) => {
                let (mut instance, new_inst) = locked
                    .take()
                    .map(|r| Ok((r, false)))
                    .unwrap_or_else(|| self.module.new_instance().map(|i| (i, true)))?;

                let result = f(&*self.module, &mut *instance, self.version.as_ref(), ext);
                if let Err(e) = &result {
                    if new_inst {
                        tracing::warn!(
                            target: "wasm-runtime",
                            error = %e,
                            "Fresh runtime instance failed",
                        )
                    } else {
                        tracing::warn!(
                            target: "wasm-runtime",
                            error = %e,
                            "Evicting failed runtime instance",
                        );
                    }
                } else {
                    *locked = Some(instance);

                    if new_inst {
                        tracing::debug!(
                            target: "wasm-runtime",
                            "Allocated WASM instance {}/{}",
                            index + 1,
                            self.instances.len(),
                        );
                    }
                }

                result
            }
            None => {
                tracing::warn!(target: "wasm-runtime", "Ran out of free WASM instances");

                // Allocate a new instance
                let mut instance = self.module.new_instance()?;

                f(&*self.module, &mut *instance, self.version.as_ref(), ext)
            }
        }
    }
}

/// Cache for the runtimes.
///
/// When an instance is requested for the first time it is added to this cache. Metadata is kept
/// with the instance so that it can be efficiently reinitialized.
///
/// When using the Wasmi interpreter execution method, the metadata includes the initial memory and
/// values of mutable globals. Follow-up requests to fetch a runtime return this one instance with
/// the memory reset to the initial memory. So, one runtime instance is reused for every fetch
/// request.
///
/// The size of cache is configurable via the cli option `--runtime-cache-size`.
pub struct RuntimeCache {
    /// A cache of runtimes along with metadata.
    ///
    /// Runtimes sorted by recent usage. The most recently used is at the front.
    runtimes: Mutex<LruMap<VersionedRuntimeId, Arc<VersionedRuntime>>>,
    /// The size of the instances cache for each runtime.
    max_runtime_instances: usize,
    cache_path: Option<PathBuf>,
}

impl RuntimeCache {
    /// Creates a new instance of a runtimes cache.
    ///
    /// `max_runtime_instances` specifies the number of instances per runtime preserved in an
    /// in-memory cache.
    ///
    /// `cache_path` allows to specify an optional directory where the executor can store files
    /// for caching.
    ///
    /// `runtime_cache_size` specifies the number of different runtimes versions preserved in an
    /// in-memory cache, must always be at least 1.
    pub fn new(
        max_runtime_instances: usize,
        cache_path: Option<PathBuf>,
        runtime_cache_size: u8,
    ) -> RuntimeCache {
        let cap = ByLength::new(runtime_cache_size.max(1) as u32);
        RuntimeCache {
            runtimes: Mutex::new(LruMap::new(cap)),
            max_runtime_instances,
            cache_path,
        }
    }

    /// Returns the cached runtime version without acquiring an instance.
    pub(crate) fn cached_runtime_version(
        &self,
        runtime_code: &RuntimeCode,
        wasm_method: WasmExecutionMethod,
        heap_alloc_strategy: HeapAllocStrategy,
    ) -> Option<Result<RuntimeVersion, Error>> {
        let versioned_runtime_id = VersionedRuntimeId {
            code_hash: runtime_code.hash.clone(),
            wasm_method,
            heap_alloc_strategy,
        };

        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes.get(&versioned_runtime_id).cloned();
        drop(runtimes);

        runtime.map(|runtime| {
            runtime
                .version
                .clone()
                .ok_or_else(|| Error::ApiError("Unknown version".into()))
        })
    }

    /// Prepares a WASM module instance and executes given function for it.
    ///
    /// This uses internal cache to find available instance or create a new one.
    /// # Parameters
    ///
    /// `runtime_code` - The runtime wasm code used setup the runtime.
    ///
    /// `ext` - The externalities to access the state.
    ///
    /// `wasm_method` - Type of WASM backend to use.
    ///
    /// `heap_alloc_strategy` - The heap allocation strategy to use.
    ///
    /// `allow_missing_func_imports` - Ignore missing function imports.
    ///
    /// `f` - Function to execute.
    ///
    /// `H` - A compile-time list of host functions to expose to the runtime.
    ///
    /// # Returns result of `f` wrapped in an additional result.
    /// In case of failure one of two errors can be returned:
    ///
    /// `Err::RuntimeConstruction` is returned for runtime construction issues.
    ///
    /// `Error::InvalidMemoryReference` is returned if no memory export with the
    /// identifier `memory` can be found in the runtime.
    pub fn with_instance<'c, H, R, F>(
        &self,
        runtime_code: &'c RuntimeCode<'c>,
        ext: &mut dyn Externalities,
        wasm_method: WasmExecutionMethod,
        heap_alloc_strategy: HeapAllocStrategy,
        allow_missing_func_imports: bool,
        f: F,
    ) -> Result<Result<R, Error>, Error>
    where
        H: HostFunctions,
        F: FnOnce(
            &dyn WasmModule,
            &mut dyn WasmInstance,
            Option<&RuntimeVersion>,
            &mut dyn Externalities,
        ) -> Result<R, Error>,
    {
        let code_hash = &runtime_code.hash;

        let versioned_runtime_id = VersionedRuntimeId {
            code_hash: code_hash.clone(),
            heap_alloc_strategy,
            wasm_method,
        };

        let mut runtimes = self.runtimes.lock(); // this must be released prior to calling f
        let versioned_runtime = if let Some(versioned_runtime) = runtimes.get(&versioned_runtime_id)
        {
            versioned_runtime.clone()
        } else {
            let code = runtime_code
                .fetch_runtime_code()
                .ok_or(WasmError::CodeNotFound)?;

            let time = std::time::Instant::now();

            let result = create_versioned_wasm_runtime::<H>(
                &code,
                ext,
                wasm_method,
                heap_alloc_strategy,
                allow_missing_func_imports,
                self.max_runtime_instances,
                self.cache_path.as_deref(),
            );

            match result {
                Ok(ref result) => {
                    tracing::debug!(
                        target: "wasm-runtime",
                        "Prepared new runtime version {:?} in {} ms.",
                        result.version,
                        time.elapsed().as_millis(),
                    );
                }
                Err(ref err) => {
                    tracing::warn!(target: "wasm-runtime", error = ?err, "Cannot create a runtime");
                }
            }

            let versioned_runtime = Arc::new(result?);

            // Save new versioned wasm runtime in cache
            runtimes.insert(versioned_runtime_id, versioned_runtime.clone());

            versioned_runtime
        };

        // Lock must be released prior to calling f
        drop(runtimes);

        Ok(versioned_runtime.with_instance(ext, f))
    }
}

/// Create a wasm runtime with the given `code`.
pub fn create_wasm_runtime_with_code<H>(
    wasm_method: WasmExecutionMethod,
    heap_alloc_strategy: HeapAllocStrategy,
    blob: RuntimeBlob,
    allow_missing_func_imports: bool,
    cache_path: Option<&Path>,
) -> Result<Box<dyn WasmModule>, WasmError>
where
    H: HostFunctions,
{
    if let Some(blob) = blob.as_polkavm_blob() {
        return sc_executor_polkavm::create_runtime::<H>(blob);
    }

    match wasm_method {
        WasmExecutionMethod::Compiled {
            instantiation_strategy,
        } => sc_executor_wasmtime::create_runtime::<H>(
            blob,
            sc_executor_wasmtime::Config {
                allow_missing_func_imports,
                cache_path: cache_path.map(ToOwned::to_owned),
                semantics: sc_executor_wasmtime::Semantics {
                    heap_alloc_strategy,
                    instantiation_strategy,
                    deterministic_stack_limit: None,
                    canonicalize_nans: false,
                    parallel_compilation: true,
                    wasm_multi_value: false,
                    wasm_bulk_memory: false,
                    wasm_reference_types: false,
                    wasm_simd: false,
                },
            },
        )
        .map(|runtime| -> Box<dyn WasmModule> { Box::new(runtime) }),
    }
}

fn decode_version(mut version: &[u8]) -> Result<RuntimeVersion, WasmError> {
    Decode::decode(&mut version).map_err(|_| {
        WasmError::Instantiation(
            "failed to decode \"Core_version\" result using old runtime version".into(),
        )
    })
}

fn decode_runtime_apis(apis: &[u8]) -> Result<Vec<([u8; 8], u32)>, WasmError> {
    use sp_api::RUNTIME_API_INFO_SIZE;

    apis.chunks(RUNTIME_API_INFO_SIZE)
        .map(|chunk| {
            // `chunk` can be less than `RUNTIME_API_INFO_SIZE` if the total length of `apis`
            // doesn't completely divide by `RUNTIME_API_INFO_SIZE`.
            <[u8; RUNTIME_API_INFO_SIZE]>::try_from(chunk)
                .map(sp_api::deserialize_runtime_api_info)
                .map_err(|_| WasmError::Other("a clipped runtime api info declaration".to_owned()))
        })
        .collect::<Result<Vec<_>, WasmError>>()
}

/// Take the runtime blob and scan it for the custom wasm sections containing the version
/// information and construct the `RuntimeVersion` from them.
///
/// If there are no such sections, it returns `None`. If there is an error during decoding those
/// sections, `Err` will be returned.
pub fn read_embedded_version(blob: &RuntimeBlob) -> Result<Option<RuntimeVersion>, WasmError> {
    if let Some(mut version_section) = blob.custom_section_contents("runtime_version") {
        let apis = blob
            .custom_section_contents("runtime_apis")
            .map(decode_runtime_apis)
            .transpose()?
            .map(Into::into);

        let core_version = apis.as_ref().and_then(sp_version::core_version_from_apis);
        // We do not use `RuntimeVersion::decode` here because that `decode_version` relies on
        // presence of a special API in the `apis` field to treat the input as a non-legacy version.
        // However the structure found in the `runtime_version` always contain an empty `apis`
        // field. Therefore the version read will be mistakenly treated as an legacy one.
        let mut decoded_version = sp_version::RuntimeVersion::decode_with_version_hint(
            &mut version_section,
            core_version,
        )
        .map_err(|_| WasmError::Instantiation("failed to decode version section".into()))?;

        if let Some(apis) = apis {
            decoded_version.apis = apis;
        }

        Ok(Some(decoded_version))
    } else {
        Ok(None)
    }
}

fn create_versioned_wasm_runtime<H>(
    code: &[u8],
    ext: &mut dyn Externalities,
    wasm_method: WasmExecutionMethod,
    heap_alloc_strategy: HeapAllocStrategy,
    allow_missing_func_imports: bool,
    max_instances: usize,
    cache_path: Option<&Path>,
) -> Result<VersionedRuntime, WasmError>
where
    H: HostFunctions,
{
    // The incoming code may be actually compressed. We decompress it here and then work with
    // the uncompressed code from now on.
    let blob = sc_executor_common::runtime_blob::RuntimeBlob::uncompress_if_needed(code)?;

    // Use the runtime blob to scan if there is any metadata embedded into the wasm binary
    // pertaining to runtime version. We do it before consuming the runtime blob for creating the
    // runtime.
    let mut version = read_embedded_version(&blob)?;

    let runtime = create_wasm_runtime_with_code::<H>(
        wasm_method,
        heap_alloc_strategy,
        blob,
        allow_missing_func_imports,
        cache_path,
    )?;

    // If the runtime blob doesn't embed the runtime version then use the legacy version query
    // mechanism: call the runtime.
    if version.is_none() {
        // Call to determine runtime version.
        let version_result = {
            // `ext` is already implicitly handled as unwind safe, as we store it in a global
            // variable.
            let mut ext = AssertUnwindSafe(ext);

            // The following unwind safety assertion is OK because if the method call panics, the
            // runtime will be dropped.
            let runtime = AssertUnwindSafe(runtime.as_ref());
            crate::executor::with_externalities_safe(&mut **ext, move || {
                runtime.new_instance()?.call("Core_version", &[])
            })
            .map_err(|_| WasmError::Instantiation("panic in call to get runtime version".into()))?
        };

        if let Ok(version_buf) = version_result {
            version = Some(decode_version(&version_buf)?)
        }
    }

    let mut instances = Vec::with_capacity(max_instances);
    instances.resize_with(max_instances, || Mutex::new(None));

    Ok(VersionedRuntime {
        module: runtime,
        version,
        instances,
    })
}
#[cfg(test)]
pub(crate) mod tests {
    use super::{
        Error, RuntimeCache, RuntimeVersion, VersionedRuntime, VersionedRuntimeId,
        WasmExecutionMethod,
    };
    use crate::{RuntimeVersionOf, executor::WasmExecutor};
    use codec::Encode;
    use sc_executor_common::wasm_runtime::{HeapAllocStrategy, WasmInstance, WasmModule};
    use sp_core::traits::{RuntimeCode, WrappedRuntimeCode};
    use sp_io::TestExternalities;
    use std::{
        borrow::Cow,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    struct CountingModule {
        new_instance_calls: Arc<AtomicUsize>,
    }

    impl WasmModule for CountingModule {
        fn new_instance(&self) -> Result<Box<dyn WasmInstance>, Error> {
            self.new_instance_calls.fetch_add(1, Ordering::SeqCst);
            Err(Error::Other("new_instance called".into()))
        }
    }

    pub(crate) fn runtime_code(hash: &[u8]) -> RuntimeCode<'static> {
        let mut runtime_code = RuntimeCode::empty();
        runtime_code.hash = hash.to_vec();
        runtime_code
    }

    pub(crate) fn insert_cached_runtime(
        cache: &RuntimeCache,
        code_hash: &[u8],
        wasm_method: WasmExecutionMethod,
        heap_alloc_strategy: HeapAllocStrategy,
        version: Option<RuntimeVersion>,
        new_instance_calls: Arc<AtomicUsize>,
    ) {
        cache.runtimes.lock().insert(
            VersionedRuntimeId {
                code_hash: code_hash.to_vec(),
                wasm_method,
                heap_alloc_strategy,
            },
            Arc::new(VersionedRuntime {
                module: Box::new(CountingModule { new_instance_calls }),
                version,
                instances: Vec::new(),
            }),
        );
    }

    #[test]
    fn cached_runtime_version_returns_known_version_without_new_instance() {
        let cache = RuntimeCache::new(1, None, 1);
        let code = runtime_code(&[1, 2, 3]);
        let wasm_method = WasmExecutionMethod::default();
        let heap_alloc_strategy = HeapAllocStrategy::Static { extra_pages: 1 };
        let new_instance_calls = Arc::new(AtomicUsize::new(0));
        let version = RuntimeVersion {
            spec_name: "cached".into(),
            ..Default::default()
        };

        insert_cached_runtime(
            &cache,
            &code.hash,
            wasm_method,
            heap_alloc_strategy,
            Some(version.clone()),
            new_instance_calls.clone(),
        );

        match cache.cached_runtime_version(&code, wasm_method, heap_alloc_strategy) {
            Some(Ok(actual)) => assert_eq!(actual, version),
            other => panic!("expected cached runtime version, got {other:?}"),
        }
        assert_eq!(new_instance_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cached_runtime_version_reports_unknown_without_new_instance() {
        let cache = RuntimeCache::new(1, None, 1);
        let code = runtime_code(&[1, 2, 3]);
        let wasm_method = WasmExecutionMethod::default();
        let heap_alloc_strategy = HeapAllocStrategy::Static { extra_pages: 1 };
        let new_instance_calls = Arc::new(AtomicUsize::new(0));

        insert_cached_runtime(
            &cache,
            &code.hash,
            wasm_method,
            heap_alloc_strategy,
            None,
            new_instance_calls.clone(),
        );

        let error = cache
            .cached_runtime_version(&code, wasm_method, heap_alloc_strategy)
            .expect("expected an exact cache hit")
            .expect_err("expected cached absence to report an unknown version");
        match error {
            Error::ApiError(message) => assert_eq!(message.to_string(), "Unknown version"),
            other => panic!("expected unknown version error, got {other:?}"),
        }
        assert_eq!(new_instance_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cached_runtime_version_requires_exact_cache_key() {
        let cache = RuntimeCache::new(1, None, 1);
        let code = runtime_code(&[1, 2, 3]);
        let wasm_method = WasmExecutionMethod::default();
        let other_wasm_method = WasmExecutionMethod::Compiled {
            instantiation_strategy: sc_executor_wasmtime::InstantiationStrategy::RecreateInstance,
        };
        let heap_alloc_strategy = HeapAllocStrategy::Static { extra_pages: 1 };
        let other_heap_alloc_strategy = HeapAllocStrategy::Dynamic {
            maximum_pages: Some(1),
        };
        let new_instance_calls = Arc::new(AtomicUsize::new(0));

        insert_cached_runtime(
            &cache,
            &code.hash,
            wasm_method,
            heap_alloc_strategy,
            Some(Default::default()),
            new_instance_calls.clone(),
        );

        assert!(
            cache
                .cached_runtime_version(&runtime_code(&[4, 5, 6]), wasm_method, heap_alloc_strategy)
                .is_none()
        );
        assert!(
            cache
                .cached_runtime_version(&code, other_wasm_method, heap_alloc_strategy)
                .is_none()
        );
        assert!(
            cache
                .cached_runtime_version(&code, wasm_method, other_heap_alloc_strategy)
                .is_none()
        );
        assert_eq!(new_instance_calls.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn runtime_version_miss_returns_embedded_version() {
        let wasm = wat::parse_str(r#"(module (memory (export "memory") 1))"#)
            .expect("minimal WAT should parse");
        let expected = RuntimeVersion {
            spec_name: "embedded-spec".into(),
            impl_name: "embedded-impl".into(),
            transaction_version: 1,
            ..Default::default()
        };
        let wasm = sp_version::embed::embed_runtime_version(&wasm, expected.clone())
            .expect("runtime version should embed");
        let code_fetcher = WrappedRuntimeCode(Cow::Owned(wasm));
        let runtime_code = RuntimeCode {
            code_fetcher: &code_fetcher,
            heap_pages: None,
            hash: vec![0xeb, 0xed, 0x00, 0x01],
        };
        let executor = WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build();
        let mut ext = TestExternalities::default();
        let actual = RuntimeVersionOf::runtime_version(&executor, &mut ext.ext(), &runtime_code)
            .expect("embedded runtime version should be returned");

        assert_eq!(actual, expected);
    }

    #[test]
    fn runtime_version_miss_returns_legacy_core_version() {
        let expected = RuntimeVersion {
            spec_name: "legacy-spec".into(),
            impl_name: "legacy-impl".into(),
            transaction_version: 1,
            ..Default::default()
        };
        let encoded = expected.encode();
        let data = encoded
            .iter()
            .map(|byte| format!(r#"\{byte:02x}"#))
            .collect::<String>();
        let wat = format!(
            r#"(module
                (memory (export "memory") 1)
                (global (export "__heap_base") i32 (i32.const 1024))
                (data (i32.const 0) "{data}")
                (func (export "Core_version") (param i32 i32) (result i64)
                    i64.const {}
                )
            )"#,
            (encoded.len() as u64) << 32,
        );
        let wasm = wat::parse_str(wat).expect("valid legacy runtime WAT");
        let code = WrappedRuntimeCode(Cow::Owned(wasm));
        let runtime_code = RuntimeCode {
            code_fetcher: &code,
            heap_pages: None,
            hash: vec![0xde, 0xad, 0xbe, 0xef],
        };
        let executor = crate::WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build();
        let mut ext = sp_io::TestExternalities::default();

        let actual =
            crate::RuntimeVersionOf::runtime_version(&executor, &mut ext.ext(), &runtime_code)
                .expect("legacy Core_version decodes");

        assert_eq!(actual, expected);
    }
}
