(module
  (import "env" "foo" (func $foo))

  ;; Declare a global.
  (global $counter (mut i32) (i32.const 1))

  (func $i32.add (export "i32.add") (param i32 i32) (result i32)
    local.get 0
    local.get 1
    i32.add
  )
  (func (param $arg i32)
    (local $tmp i32)

    global.get 0
    i32.const 1
    i32.add
    local.tee $tmp
    global.set $counter

    local.get $tmp
    local.get $arg
    call $i32.add
    drop
  )
)
