(module
  (type (;0;) (func (param i32 i32)))
  (type (;1;) (func))
  (import "env" "ext_return" (func $ext_return (;0;) (type 0)))
  (import "env" "memory" (memory (;0;) 1 1))
  (global (;0;) (mut i32) i32.const 0)
  (export "call" (func 4))
  (start 3)
  (func $start (;1;) (type 1)
    (local i32)
  )
  (func (;2;) (type 1))
  (func (;3;) (type 1)
    global.get 0
    i32.const 7
    i32.add
    global.set 0
    global.get 0
    i32.const 1024
    i32.gt_u
    if ;; label = @1
      unreachable
    end
    call $start
    global.get 0
    i32.const 7
    i32.sub
    global.set 0
  )
  (func (;4;) (type 1)
    global.get 0
    i32.const 6
    i32.add
    global.set 0
    global.get 0
    i32.const 1024
    i32.gt_u
    if ;; label = @1
      unreachable
    end
    call 2
    global.get 0
    i32.const 6
    i32.sub
    global.set 0
  )
)
