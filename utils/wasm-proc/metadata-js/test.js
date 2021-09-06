// We use the assert standard library to make assertions
const assert = require('assert');
const fs = require("fs");
const path = require("path");
const wasmMetadata = require(".");

let wasmBytes = fs.readFileSync(
    path.join(__dirname, "../../../examples/target/wasm32-unknown-unknown/release/demo_meta.meta.wasm")
);

wasmMetadata.getWasmMetadata(wasmBytes).then(metadata => {
    assert.deepStrictEqual(
        metadata,
        {
            init_input: "MessageInitIn",
            init_output: "MessageInitOut",
            input: "MessageIn",
            output: "MessageOut",
            title: 'Example program with metadata',
            types: {
                "MessageInitIn": {
                    "currency": "String",
                    "amount": "u8"
                },
                "MessageInitOut": {
                    "exchange_rate": "Result<u8,u8>",
                    "sum": "u8"
                },
                "MessageIn": {
                    "id": "Id"
                },
                "MessageOut": {
                    "res": "Vec<Result<Wallet,String>>"
                },
                "Id": {
                    "decimal": "u64",
                    "hex": "Vec<u8>"
                },
                "Wallet": {
                    "id": "Id",
                    "person": "Person"
                },
                "Person": {
                    "surname": "String",
                    "name": "String",
                    "patronymic": "Option<String>"
                }
            }
        }
    )
});
