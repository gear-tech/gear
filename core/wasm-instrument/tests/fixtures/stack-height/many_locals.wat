(module
	(func $one-group-many-locals
		(local i64) (local i64) (local i32)
	)
	(func $main
		(call
			$one-group-many-locals
		)
	)
)
