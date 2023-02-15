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
            gas: () => { },
            gr_debug: (string_ptr, len) => {
                console.log('GR_DEBUG', len, ab2str(memory.buffer.slice(string_ptr, string_ptr + len)));
            },
            gr_panic: (string_ptr, len) => {
                console.log('GR_PANIC', len, ab2str(memory.buffer.slice(string_ptr, string_ptr + len)));
            },
            gr_oom_panic: () => { },
            gr_block_height: () => { },
            gr_block_timestamp: () => { },
            gr_exit: () => { },
            gr_status_code: () => { },
            gr_program_id: () => { },
            gr_message_id: () => { },
            gr_size: () => { },
            gr_read: () => { },
            gr_source: () => { },
            gr_gas_available: () => { },
            gr_send: () => { },
            gr_send_wgas: () => { },
            gr_send_commit: () => { },
            gr_send_commit_wgas: () => { },
            gr_send_init: () => { },
            gr_send_push: () => { },
            gr_reply: () => { },
            gr_reply_wgas: () => { },
            gr_reply_push: () => { },
            gr_reply_commit: () => { },
            gr_reply_commit_wgas: () => { },
            gr_reply_to: () => { },
            gr_value: () => { },
            gr_wait: () => { },
            gr_wait_for: () => { },
            gr_wait_up_to: () => { },
            gr_wake: () => { },
            gr_error: () => { },
        }
    };

    let metadata = {
        init_input: null,
        init_output: null,
        async_init_input: null,
        async_init_output: null,
        handle_input: null,
        handle_output: null,
        async_handle_input: null,
        async_handle_output: null,
        state_input: null,
        state_output: null,
        registry: null,
        title: null,
    }

    let module = await WebAssembly.instantiate(wasmBytes, importObj);
    let instance_exports = module.instance.exports;

    const expected_exports = [
        "init_input", "init_output",
        "async_init_input", "async_init_output",
        "handle_input", "handle_output",
        "async_handle_input", "async_handle_output",
        "state_input", "state_output",
        "registry",
        "title",
    ];

    for (const exp of expected_exports) {
        if (`meta_${exp}` in instance_exports)
            metadata[exp] = readMeta(memory, instance_exports[`meta_${exp}`]());
    }

    if (metadata.registry !== null)
        metadata.registry = `0x${metadata.registry}`;

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
