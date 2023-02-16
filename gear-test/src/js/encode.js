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
    .option('json', {
        alias: 'j',
        description: 'Json with data for parse',
    })
    .help()
    .alias('help', 'h')
    .argv;

let wasmBytes = fs.readFileSync(argv.path);
let json = argv.json;
let findingType = argv.type;

getWasmMetadata(wasmBytes).then( meta => {
    let type = meta[findingType];
    let encoded = CreateType.create(type, json, meta.types);
    process.stdout.write(encoded.toHex().slice(2));
});
