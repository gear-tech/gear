(module
	(func $fibonacci_with_break (result i32)
		(local $x i32) (local $y i32)

		(block $unrolled_loop
			(local.set $x (i32.const 0))
			(local.set $y (i32.const 1))

			local.get $x
			local.get $y
			local.tee $x
			i32.add
			local.set $y

			i32.const 1
			br_if $unrolled_loop

			local.get $x
			local.get $y
			local.tee $x
			i32.add
			local.set $y
		)

		local.get $y
	)
)
