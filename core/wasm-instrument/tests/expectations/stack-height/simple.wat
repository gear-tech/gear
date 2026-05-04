(module
  (type (;0;) (func))
  (global (;0;) (mut i32) i32.const 0)
  (export "simple" (func 1))
  (func (;0;) (type 0)
    i32.const 123
    drop
  )
  (func (;1;) (type 0)
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
    call 0
    global.get 0
    i32.const 7
    i32.sub
    global.set 0
  )
)
