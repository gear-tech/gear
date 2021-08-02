# wasm-proc

### Install

To install, checkout current repository and:

```
cd utils/wasm-proc
cargo install --path ./
```

### Use

To process cargo artifact, just pass it to the `wasm-proc`!

```
wasm_proc somefile.wasm
```

You will get two files in the same directory:
- `somefile.opt.wasm` which is destined for the node.
- `somefile.meta.wasm` which can be used by the browser or another ui to aquire metadata for the main wasm.
