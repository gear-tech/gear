(module
  (import "env" "foo" (func $foo))
  (func (param i32)
    local.get 0
    i32.const 0
    call $i32.add
    drop
  )
  (func $i32.add (export "i32.add") (param i32 i32) (result i32)
    local.get 0
    local.get 1
    i32.add
  )
  (table 10 funcref)

  ;; Refer all types of functions: imported, defined not exported and defined exported.
  (elem (i32.const 0) 0 1 2)
)
