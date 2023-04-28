(module
    (import "env" "memory" (memory 0))
    (export "handle" (func $handle))
    (func $handle
        (loop $my_loop
            i32.const 1
            br_if $my_loop
        )
    )
)
