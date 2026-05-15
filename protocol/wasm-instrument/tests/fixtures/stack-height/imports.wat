;; This test 

(module
  (import "env" "foo" (func $foo))
  (import "env" "boo" (func $boo))

  (func (export "i32.add") (param i32 i32) (result i32)
    call $foo
    call $boo

    local.get 0
    local.get 1
    i32.add
  )
)
