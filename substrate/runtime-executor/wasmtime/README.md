# sc-executor-wasmtime

Defines a `WasmRuntime` that uses the Wasmtime JIT to execute.

Local Cargo package name: `sc-executor-wasmtime`.
Gear publish name: `gsc-executor-wasmtime`.

License: GPL-3.0-or-later WITH Classpath-exception-2.0.

Source: Parity Polkadot SDK `stable2409` at `298f676c91d64f15f38ea7fd78f125c5889ab09c`.

Copied source files retain the upstream SPDX headers and Parity Technologies copyright notices. Gear-authored files are marked in their file headers. Gear maintains local changes to isolate the remaining Polkadot SDK fork delta and publishes this crate under the `gsc-executor-wasmtime` package name for Gear ecosystem crates.

`src/test-guard-page-skip.wat` is a modified WebAssembly testsuite fixture from <https://github.com/WebAssembly/testsuite/blob/01efde81028c5b0d099eb836645a2dc5e7755449/skip-stack-guard-page.wast>, licensed under Apache-2.0: <https://github.com/WebAssembly/testsuite/blob/01efde81028c5b0d099eb836645a2dc5e7755449/LICENSE>.

GPL-3.0-or-later license text: <https://www.gnu.org/licenses/gpl-3.0.html>.
Classpath exception 2.0 text: <https://spdx.org/licenses/Classpath-exception-2.0.html>.
