(module
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32)))
  (import "env" "gas" (func (;0;) (type 1)))
  (func $fibonacci_with_break (;1;) (type 0) (result i32)
    (local i32 i32)
    i32.const 15
    call 0
    block ;; label = @1
      i32.const 0
      local.set 0
      i32.const 1
      local.set 1
      local.get 0
      local.get 1
      local.tee 0
      i32.add
      local.set 1
      i32.const 1
      br_if 0 (;@1;)
      i32.const 5
      call 0
      local.get 0
      local.get 1
      local.tee 0
      i32.add
      local.set 1
    end
    local.get 1
  )
)
