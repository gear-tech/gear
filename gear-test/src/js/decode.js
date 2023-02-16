const fs = require("fs");
const { CreateType, getWasmMetadata } = require("@gear-js/api");
const yargs = require("yargs");

const argv = yargs
    .option('path', {
        alias: 'p',
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
    let encoded = CreateType.create('bytes', '0x' + String(argv.bytes))
    let decoded = CreateType.create(type, encoded, meta.types);
    process.stdout.write(JSON.stringify(decoded));
});
