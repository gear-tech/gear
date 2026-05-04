(module
  (type (;0;) (func))
  (global (;0;) (mut i32) i32.const 0)
  (func $one-group-many-locals (;0;) (type 0)
    (local i64 i64 i32)
  )
  (func $main (;1;) (type 0)
    global.get 0
    i32.const 5
    i32.add
    global.set 0
    global.get 0
    i32.const 1024
    i32.gt_u
    if ;; label = @1
      unreachable
    end
    call $one-group-many-locals
    global.get 0
    i32.const 5
    i32.sub
    global.set 0
  )
)
