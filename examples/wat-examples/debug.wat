(module
    (import "env" "memory" (memory 0x100))
    (import "env" "gr_debug" (func $debug (param i32 i32)))
    (export "init" (func $init))
    (func $init
        (loop
            i32.const 0
            i32.const 0xFF0000
            call $debug
            br 0
        )
    )
    (data (;0;) (i32.const 0) "LOL KEK LOL")
)
