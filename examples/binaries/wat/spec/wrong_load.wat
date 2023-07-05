(module
    (import "env" "memory" (memory 1))
    (export "init" (func $init))
    (func $init
        i32.const 0x10000
        i32.load
        drop
    )
)
