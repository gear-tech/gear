(module
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32)))
  (import "env" "gas" (func (;0;) (type 1)))
  (func (;1;) (type 0) (param i32) (result i32)
    i32.const 2
    call 0
    i32.const 1
    if (result i32) ;; label = @1
      i32.const 3
      call 0
      local.get 0
      i32.const 1
      i32.add
    else
      i32.const 2
      call 0
      local.get 0
      i32.popcnt
    end
  )
)
