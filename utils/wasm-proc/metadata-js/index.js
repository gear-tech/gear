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
            gr_msg_id: () => { },
            gr_size: () => { },
            gr_read: () => { },
            gr_source: () => { },
            gr_gas_available: () => { },
            gr_send: () => { },
            gr_send_commit: () => { },
            gr_send_init: () => { },
            gr_send_push: () => { },
            gr_reply: () => { },
            gr_reply_push: () => { },
            gr_reply_to: () => { },
            gr_value: () => { },
            gr_wait: () => { },
            gr_wake: () => { },
        }
    };

    let metadata = {
        init_input: "",
        init_output: "",
        async_init_input: "",
        async_init_output: "",
        handle_input: "",
        handle_output: "",
        async_handle_input: "",
        async_handle_output: "",
        state_input: "",
        state_output: "",
        registry: "",
        title: ""
    }

    let module = await WebAssembly.instantiate(wasmBytes, importObj);

    metadata.init_input = readMeta(memory, module.instance.exports.meta_init_input());
    metadata.init_output = readMeta(memory, module.instance.exports.meta_init_output());
    metadata.async_init_input = readMeta(memory, module.instance.exports.meta_async_init_input());
    metadata.async_init_output = readMeta(memory, module.instance.exports.meta_async_init_output());
    metadata.handle_input = readMeta(memory, module.instance.exports.meta_handle_input());
    metadata.handle_output = readMeta(memory, module.instance.exports.meta_handle_output());
    metadata.async_handle_input = readMeta(memory, module.instance.exports.meta_async_handle_input());
    metadata.async_handle_output = readMeta(memory, module.instance.exports.meta_async_handle_output());
    metadata.state_input = readMeta(memory, module.instance.exports.meta_state_input());
    metadata.state_output = readMeta(memory, module.instance.exports.meta_state_output());
    metadata.registry = `0x${readMeta(memory, module.instance.exports.meta_registry())}`;
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
