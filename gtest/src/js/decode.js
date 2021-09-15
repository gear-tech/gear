const fs = require("fs");
const { CreateType, getWasmMetadata } = require("gear-js-lib");
const yargs = require("yargs");

const argv = yargs
    .option('path', {
        alias: 'p',
        default: "../../../examples/target/wasm32-unknown-unknown/release/demo_meta.meta.wasm",
        description: 'Specifies the path to .meta.wasm binary',
    })
    .option('type', {
        alias: 't',
        description: 'Finding type bytes',
    })
    .option('bytes', {
        alias: 'b',
        description: 'Codec bytes',
    })
    .help()
    .alias('help', 'h')
    .argv;

let wasmBytes = fs.readFileSync(argv.path);
let findingType = argv.type;

getWasmMetadata(wasmBytes).then( meta => {
    let type = meta[findingType];
    let encoded = CreateType.encode('bytes', '0x' + String(argv.bytes))
    let decoded = CreateType.decode(type, encoded, meta);
    process.stdout.write(JSON.stringify(decoded));
});
