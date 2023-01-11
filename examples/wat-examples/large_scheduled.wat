(module
    (import "env" "memory" (memory 1))
    (import "env" "gr_reply_push" (func $reply_push (param i32 i32 i32)))
    (import "env" "gr_reply_commit" (func $reply_commit (param i32 i32 i32)))
    (export "init" (func $init))
    ;; ptr [0; 4) for gr_reply_push err length
    ;; ptr [4; 40) for gr_reply_commit err length
    ;; length of each push supposed to be a half of a wasm page (32 kb)
    ;; delay for sending will be 100
    (func $init
        ;; for example pushing 3 fullfilled wasm pages
        (call $reply_push (i32.const 0) (i32.const 0x8000) (i32.const 0))
        (call $reply_push (i32.const 0) (i32.const 0x8000) (i32.const 0))
        (call $reply_push (i32.const 0) (i32.const 0x8000) (i32.const 0))
        (call $reply_push (i32.const 0) (i32.const 0x8000) (i32.const 0))
        (call $reply_push (i32.const 0) (i32.const 0x8000) (i32.const 0))
        (call $reply_push (i32.const 0) (i32.const 0x8000) (i32.const 0))

        ;; panicking if something went wrong
        i32.const 0
        i32.load
        i32.eqz
        br_if 0
        unreachable

        ;; sending commit
        (call $reply_commit (i32.const 0) (i32.const 100) (i32.const 4))

        ;; panicking if something went wrong
        i32.const 4
        i32.load
        i32.eqz
        br_if 0
        unreachable
    )
)
