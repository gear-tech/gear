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

let wasmBytes = fs.readFileSync(path.join(targetDir, "demo_meta.meta.wasm"));

wasmMetadata.getWasmMetadata(wasmBytes).then(metadata => {
    const reg = new TypeRegistry();
    metadata.registry = JSON.stringify(reg.createType('PortableRegistry', metadata.registry).toHuman());

    assert.deepStrictEqual(
        metadata,
        {
            init_input: "MessageInitIn",
            init_output: "MessageInitOut",
            async_init_input: "MessageInitAsyncIn",
            async_init_output: "MessageInitAsyncOut",
            handle_input: "MessageIn",
            handle_output: "MessageOut",
            async_handle_input: "MessageHandleAsyncIn",
            async_handle_output: "MessageHandleAsyncOut",
            state_input: "Option<Id>",
            state_output: "Vec<Wallet>",
            registry: '{"types":[{"id":"0","type":{"path":["demo_meta","MessageInitIn"],"params":[],"def":{"Composite":{"fields":[{"name":"amount","type":"1","typeName":"u8","docs":[]},{"name":"currency","type":"2","typeName":"String","docs":[]}]}},"docs":[]}},{"id":"1","type":{"path":[],"params":[],"def":{"Primitive":"U8"},"docs":[]}},{"id":"2","type":{"path":[],"params":[],"def":{"Primitive":"Str"},"docs":[]}},{"id":"3","type":{"path":["demo_meta","MessageInitOut"],"params":[],"def":{"Composite":{"fields":[{"name":"exchange_rate","type":"4","typeName":"Result<u8, u8>","docs":[]},{"name":"sum","type":"1","typeName":"u8","docs":[]}]}},"docs":[]}},{"id":"4","type":{"path":["Result"],"params":[{"name":"T","type":"1"},{"name":"E","type":"1"}],"def":{"Variant":{"variants":[{"name":"Ok","fields":[{"name":null,"type":"1","typeName":null,"docs":[]}],"index":"0","docs":[]},{"name":"Err","fields":[{"name":null,"type":"1","typeName":null,"docs":[]}],"index":"1","docs":[]}]}},"docs":[]}},{"id":"5","type":{"path":["demo_meta","MessageInitAsyncIn"],"params":[],"def":{"Composite":{"fields":[{"name":"empty","type":"6","typeName":"()","docs":[]}]}},"docs":[]}},{"id":"6","type":{"path":[],"params":[],"def":{"Tuple":[]},"docs":[]}},{"id":"7","type":{"path":["demo_meta","MessageInitAsyncOut"],"params":[],"def":{"Composite":{"fields":[{"name":"empty","type":"6","typeName":"()","docs":[]}]}},"docs":[]}},{"id":"8","type":{"path":["demo_meta","MessageIn"],"params":[],"def":{"Composite":{"fields":[{"name":"id","type":"9","typeName":"Id","docs":[]}]}},"docs":[]}},{"id":"9","type":{"path":["demo_meta","Id"],"params":[],"def":{"Composite":{"fields":[{"name":"decimal","type":"10","typeName":"u64","docs":[]},{"name":"hex","type":"11","typeName":"Vec<u8>","docs":[]}]}},"docs":[]}},{"id":"10","type":{"path":[],"params":[],"def":{"Primitive":"U64"},"docs":[]}},{"id":"11","type":{"path":[],"params":[],"def":{"Sequence":{"type":"1"}},"docs":[]}},{"id":"12","type":{"path":["demo_meta","MessageOut"],"params":[],"def":{"Composite":{"fields":[{"name":"res","type":"13","typeName":"Option<Wallet>","docs":[]}]}},"docs":[]}},{"id":"13","type":{"path":["Option"],"params":[{"name":"T","type":"14"}],"def":{"Variant":{"variants":[{"name":"None","fields":[],"index":"0","docs":[]},{"name":"Some","fields":[{"name":null,"type":"14","typeName":null,"docs":[]}],"index":"1","docs":[]}]}},"docs":[]}},{"id":"14","type":{"path":["demo_meta","Wallet"],"params":[],"def":{"Composite":{"fields":[{"name":"id","type":"9","typeName":"Id","docs":[]},{"name":"person","type":"15","typeName":"Person","docs":[]}]}},"docs":[]}},{"id":"15","type":{"path":["demo_meta","Person"],"params":[],"def":{"Composite":{"fields":[{"name":"surname","type":"2","typeName":"String","docs":[]},{"name":"name","type":"2","typeName":"String","docs":[]}]}},"docs":[]}},{"id":"16","type":{"path":["demo_meta","MessageHandleAsyncIn"],"params":[],"def":{"Composite":{"fields":[{"name":"empty","type":"6","typeName":"()","docs":[]}]}},"docs":[]}},{"id":"17","type":{"path":["demo_meta","MessageHandleAsyncOut"],"params":[],"def":{"Composite":{"fields":[{"name":"empty","type":"6","typeName":"()","docs":[]}]}},"docs":[]}},{"id":"18","type":{"path":["Option"],"params":[{"name":"T","type":"9"}],"def":{"Variant":{"variants":[{"name":"None","fields":[],"index":"0","docs":[]},{"name":"Some","fields":[{"name":null,"type":"9","typeName":null,"docs":[]}],"index":"1","docs":[]}]}},"docs":[]}},{"id":"19","type":{"path":[],"params":[],"def":{"Sequence":{"type":"14"}},"docs":[]}}]}',
            title: 'Example program with metadata'
        }
    )
});

wasmBytes = fs.readFileSync(path.join(targetDir, "demo_async.meta.wasm"));

wasmMetadata.getWasmMetadata(wasmBytes).then(metadata => {
    const reg = new TypeRegistry();
    metadata.registry = JSON.stringify(reg.createType('PortableRegistry', metadata.registry).toHuman());

    assert.deepStrictEqual(
        metadata,
        {
            init_input: "Vec<u8>",
            init_output: "Vec<u8>",
            async_init_input: '',
            async_init_output: '',
            handle_input: "Vec<u8>",
            handle_output: "Vec<u8>",
            async_handle_input: '',
            async_handle_output: '',
            state_input: '',
            state_output: '',
            registry: '{"types":[{"id":"0","type":{"path":[],"params":[],"def":{"Sequence":{"type":"1"}},"docs":[]}},{"id":"1","type":{"path":[],"params":[],"def":{"Primitive":"U8"},"docs":[]}}]}',
            title: 'demo async'
        }
    )
});
