(module
    (import "env" "memory" (memory 0))
    (import "env" "alloc" (func $alloc (param i32) (result i32)))
    (export "init" (func $init))
    (func $init
        (local $i i32)

        ;; alloc 0x200 pages, so mem pages are: 0..=0x1ff
        (block
            i32.const 0x200
            call $alloc
            i32.eqz
            br_if 0
            unreachable
        )

        ;; make read and then write for all gear pages
        (loop
            local.get $i
            i32.const 1
            i32.add
            local.set $i

            local.get $i
            i32.const 0x1000
            i32.mul
            i32.load
            drop

            local.get $i
            i32.const 0x1000
            i32.mul
            i32.const 0x800
            i32.add
            i32.const 0x42
            i32.store


            local.get $i
            i32.const 0x1fff
            i32.ne
            br_if 0
        )
    )
)
