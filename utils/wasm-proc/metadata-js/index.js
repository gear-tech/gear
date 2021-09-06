exports.getWasmMetadata = async (wasmBytes) => {
    const memory = new WebAssembly.Memory({ initial: 256, maximum: 512 });
    const importObj = {
        env: {
            abortStackOverflow: () => { throw new Error('overflow'); },
            table: new WebAssembly.Table({ initial: 0, maximum: 0, element: 'anyfunc' }),
            tableBase: 0,
            memory: memory,
            memoryBase: 1024,
            STACKTOP: 0,
            STACK_MAX: memory.buffer.byteLength,
            alloc: (pages) => { return memory.grow(pages) },
            free: (_pages) => { },
            gr_debug: (msg) => { console.log(msg) },
        }
    };
    let metadata = {
        init_input: "",
        init_output: "",
        input: "",
        output: "",
        title: "",
    }

    let module = await WebAssembly.instantiate(wasmBytes, importObj);

    metadata.init_input = JSON.parse(readMeta(memory, module.instance.exports.meta_init_input()));
    metadata.init_output = JSON.parse(readMeta(memory, module.instance.exports.meta_init_output()));
    metadata.input = JSON.parse(readMeta(memory, module.instance.exports.meta_input()));
    metadata.output = JSON.parse(readMeta(memory, module.instance.exports.meta_output()));
    metadata.title = readMeta(memory, module.instance.exports.meta_title());


    return metadata;


}

function readMeta(memory, ptr) {

    let length = memory.buffer.slice(ptr + 4, ptr + 8);
    length = new Uint32Array(length)[0];

    let pointer = memory.buffer.slice(ptr, ptr + 4);
    pointer = new Uint32Array(pointer)[0];

    let buf = memory.buffer.slice(pointer, pointer + length);
    return ab2str(buf);
}

function ab2str(buf) {
    return String.fromCharCode.apply(null, new Uint8Array(buf));
}