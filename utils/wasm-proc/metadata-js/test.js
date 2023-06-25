// We use the assert standard library to make assertions
const assert = require('assert');
const fs = require("fs");
const path = require("path");
const wasmMetadata = require(".");
const { TypeRegistry } = require('@polkadot/types')

function hexIsValid(str) {
    let re = /0x[0-9a-f]+/;
    return re.test(str);
}

const targetDir = path.join(__dirname, "../../../target/wasm32-unknown-unknown/release/");
const files = fs.readdirSync(targetDir);

for (const file of files) {
    if (file.endsWith(".meta.wasm")) {
        let wasmBytes = fs.readFileSync(path.join(targetDir, file));
        wasmMetadata.getWasmMetadata(wasmBytes).then(metadata => {
            if (metadata.registry !== null && !hexIsValid(metadata.registry)) {
                console.log(`Demo ${file} has invalid hex in registry: ${metadata.registry}`);
                process.exit(1)
            }
        });
    }
}
