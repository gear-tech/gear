// We use the assert standard library to make assertions
const assert = require('assert');
const fs = require("fs");
const wasmMetadata = require(".");

let wasmBytes = fs.readFileSync("../../../examples/target/wasm32-unknown-unknown/release/demo_meta.meta.wasm");
wasmMetadata.getWasmMetadata(wasmBytes).then(metadata => {

    assert.deepStrictEqual(metadata, { input: { value: 'u64', annotation: 'String' }, output: { old_value: 'u64', new_value: 'u64' } })

});
