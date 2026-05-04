(module
    (import "env" "memory" (memory 0))
    (export "init" (func $init))
    (func $init
        call $init
    )
)
