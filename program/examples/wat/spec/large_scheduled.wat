(module
    (import "env" "memory" (memory 1))
    (import "env" "gr_send_init" (func $gr_send_init (param i32)))
    (import "env" "gr_send_push" (func $gr_send_push (param i32 i32 i32 i32)))
    (import "env" "gr_source" (func $gr_source (param i32)))
    (import "env" "gr_send_commit" (func $gr_send_commit (param i32 i32 i32 i32)))
    (export "init" (func $init))
    ;; ptr [0; 4) for gr_send_init error code
    ;; ptr [4; 8) for gr_send_init handle
    ;; ptr [8; 12) for gr_send_push error code
    ;; ptr [12; 60) for gr_source actor id and value
    ;; ptr [60; 92) for gr_send_commit err
    ;; length of each push supposed to be a half of a wasm page (32 kb)
    ;; delay for sending will be 100
    (func $init
        ;; initialize message handle
        (call $gr_send_init (i32.const 0))

        ;; panicking if something went wrong
        (block
            i32.const 0
            i32.load
            i32.eqz
            br_if 0
            unreachable
        )

        ;; for example pushing 3 fulfilled wasm pages
        (call $gr_send_push (i32.load (i32.const 4)) (i32.const 0) (i32.const 0x8000) (i32.const 8))
        (call $gr_send_push (i32.load (i32.const 4)) (i32.const 0) (i32.const 0x8000) (i32.const 8))
        (call $gr_send_push (i32.load (i32.const 4)) (i32.const 0) (i32.const 0x8000) (i32.const 8))
        (call $gr_send_push (i32.load (i32.const 4)) (i32.const 0) (i32.const 0x8000) (i32.const 8))
        (call $gr_send_push (i32.load (i32.const 4)) (i32.const 0) (i32.const 0x8000) (i32.const 8))
        (call $gr_send_push (i32.load (i32.const 4)) (i32.const 0) (i32.const 0x8000) (i32.const 8))

        ;; panicking if something went wrong
        (block
            i32.const 8
            i32.load
            i32.eqz
            br_if 0
            unreachable
        )

        ;; get actor id
        (call $gr_source (i32.const 12))

        ;; sending commit
        (call $gr_send_commit (i32.load (i32.const 4)) (i32.const 12) (i32.const 100) (i32.const 60))

        ;; panicking if something went wrong
        (block
            i32.const 60
            i32.load
            i32.eqz
            br_if 0
            unreachable
        )
    )
)
