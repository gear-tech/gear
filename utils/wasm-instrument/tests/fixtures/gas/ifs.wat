(module
	(func (param $x i32) (result i32)
		(if (result i32)
			(i32.const 1)
			(then (i32.add (local.get $x) (i32.const 1)))
			(else (i32.popcnt (local.get $x)))
		)
	)
)
