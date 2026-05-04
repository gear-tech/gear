(module
  (type (;0;) (func))
  (type (;1;) (func (param i32 i32) (result i32)))
  (import "env" "foo" (func $foo (;0;) (type 0)))
  (import "env" "boo" (func $boo (;1;) (type 0)))
  (global (;0;) (mut i32) i32.const 0)
  (export "i32.add" (func 3))
  (func (;2;) (type 1) (param i32 i32) (result i32)
    call $foo
    call $boo
    local.get 0
    local.get 1
    i32.add
  )
  (func (;3;) (type 1) (param i32 i32) (result i32)
    global.get 0
    i32.const 9
    i32.add
    global.set 0
    global.get 0
    i32.const 1024
    i32.gt_u
    if ;; label = @1
      unreachable
    end
    local.get 0
    local.get 1
    call 2
    global.get 0
    i32.const 9
    i32.sub
    global.set 0
  )
)
