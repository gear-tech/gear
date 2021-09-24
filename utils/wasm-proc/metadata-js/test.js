// We use the assert standard library to make assertions
const assert = require('assert');
const fs = require("fs");
const path = require("path");
const wasmMetadata = require(".");
const { TypeRegistry } = require('@polkadot/types')

let wasmBytes = fs.readFileSync(
    path.join(__dirname, "../../../examples/target/wasm32-unknown-unknown/release/demo_meta.meta.wasm")
);

wasmMetadata.getWasmMetadata(wasmBytes).then(metadata => {
    const reg = new TypeRegistry();
    metadata.registry = JSON.stringify(reg.createType('PortableRegistry', metadata.registry).toHuman());

    assert.deepStrictEqual(
        metadata,
        {
            init_input: "MessageInitIn",
            init_output: "MessageInitOut",
            input: "MessageIn",
            output: "MessageOut",
            registry: '{"types":[{"id":"0","type":{"path":["demo_meta","MessageInitIn"],"params":[],"def":{"Composite":{"fields":[{"name":"amount","type":"1","typeName":"u8","docs":[]},{"name":"currency","type":"2","typeName":"Vec<u8>","docs":[]}]}},"docs":[]}},{"id":"1","type":{"path":[],"params":[],"def":{"Primitive":"U8"},"docs":[]}},{"id":"2","type":{"path":[],"params":[],"def":{"Sequence":{"type":"1"}},"docs":[]}},{"id":"3","type":{"path":["demo_meta","MessageInitOut"],"params":[],"def":{"Composite":{"fields":[{"name":"exchange_rate","type":"4","typeName":"Result<u8, u8>","docs":[]},{"name":"sum","type":"1","typeName":"u8","docs":[]}]}},"docs":[]}},{"id":"4","type":{"path":["Result"],"params":[{"name":"T","type":"1"},{"name":"E","type":"1"}],"def":{"Variant":{"variants":[{"name":"Ok","fields":[{"name":null,"type":"1","typeName":null,"docs":[]}],"index":"0","docs":[]},{"name":"Err","fields":[{"name":null,"type":"1","typeName":null,"docs":[]}],"index":"1","docs":[]}]}},"docs":[]}},{"id":"5","type":{"path":["demo_meta","MessageIn"],"params":[],"def":{"Composite":{"fields":[{"name":"id","type":"6","typeName":"Id","docs":[]}]}},"docs":[]}},{"id":"6","type":{"path":["demo_meta","Id"],"params":[],"def":{"Composite":{"fields":[{"name":"decimal","type":"7","typeName":"u64","docs":[]},{"name":"hex","type":"2","typeName":"Vec<u8>","docs":[]}]}},"docs":[]}},{"id":"7","type":{"path":[],"params":[],"def":{"Primitive":"U64"},"docs":[]}},{"id":"8","type":{"path":["demo_meta","MessageOut"],"params":[],"def":{"Composite":{"fields":[{"name":"res","type":"9","typeName":"Option<Wallet>","docs":[]}]}},"docs":[]}},{"id":"9","type":{"path":["Option"],"params":[{"name":"T","type":"10"}],"def":{"Variant":{"variants":[{"name":"None","fields":[],"index":"0","docs":[]},{"name":"Some","fields":[{"name":null,"type":"10","typeName":null,"docs":[]}],"index":"1","docs":[]}]}},"docs":[]}},{"id":"10","type":{"path":["demo_meta","Wallet"],"params":[],"def":{"Composite":{"fields":[{"name":"id","type":"6","typeName":"Id","docs":[]},{"name":"person","type":"11","typeName":"Person","docs":[]}]}},"docs":[]}},{"id":"11","type":{"path":["demo_meta","Person"],"params":[],"def":{"Composite":{"fields":[{"name":"surname","type":"2","typeName":"Vec<u8>","docs":[]},{"name":"name","type":"2","typeName":"Vec<u8>","docs":[]}]}},"docs":[]}}]}',
            title: 'Example program with metadata'
        }
    )
});
