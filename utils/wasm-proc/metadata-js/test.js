// We use the assert standard library to make assertions
const assert = require('assert');
const fs = require("fs");
const path = require("path");
const wasmMetadata = require(".");

let wasmBytes = fs.readFileSync(path.join(__dirname, "../../../examples/target/wasm32-unknown-unknown/release/demo_meta.meta.wasm"));
wasmMetadata.getWasmMetadata(wasmBytes).then(metadata => {

    assert.deepStrictEqual(
        metadata,
        {
            init_input: { value: 'u64', annotation: 'String' },
            init_output: { old_value: 'u64', new_value: 'u64' },
            input: { value: 'u64', annotation: 'String' },
            output: { old_value: 'u64', new_value: 'u64' },
            title: 'Example program with metadata'
        })

});
