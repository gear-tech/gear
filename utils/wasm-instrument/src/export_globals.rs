use crate::{
    module::{Export, ModuleBuilder},
    Module,
};
use alloc::{format, vec::Vec};
use wasmparser::{ExternalKind, TypeRef};

/// Export all declared mutable globals as `prefix_index`.
///
/// This will export all internal mutable globals under the name of
/// concat(`prefix`, `"_"`, `i`) where i is the index inside the range of
/// [0..total number of internal mutable globals].
pub fn export_mutable_globals(module: Module, prefix: &str) -> Module {
    let exports = module
        .global_section()
        .map(|section| {
            section
                .iter()
                .enumerate()
                .filter_map(
                    |(index, global)| {
                        if global.ty.mutable {
                            Some(index)
                        } else {
                            None
                        }
                    },
                )
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let imports_count = module.import_count(|ty| matches!(ty, TypeRef::Global(_)));

    let mut mbuilder = ModuleBuilder::from_module(module);
    for (symbol_index, export) in exports.into_iter().enumerate() {
        mbuilder.push_export(Export {
            name: format!("{}_{}", prefix, symbol_index),
            kind: ExternalKind::Global,
            index: (imports_count + export) as u32,
        });
    }

    mbuilder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! parse_wat {
        ($module:ident = $source:expr) => {
            let module_bytes = wat::parse_str($source).unwrap();
            let $module = Module::new(&module_bytes).unwrap();
        };
    }

    macro_rules! test_export_global {
        (name = $name:ident; input = $input:expr; expected = $expected:expr) => {
            #[test]
            fn $name() {
                parse_wat!(input_module = $input);
                parse_wat!(expected_module = $expected);

                let input_module = export_mutable_globals(input_module, "exported_internal_global");

                let actual_bytes = input_module
                    .serialize()
                    .expect("injected module must have a function body");

                let expected_bytes = expected_module
                    .serialize()
                    .expect("injected module must have a function body");

                let actual_wat = wasmprinter::print_bytes(actual_bytes).unwrap();
                let expected_wat = wasmprinter::print_bytes(expected_bytes).unwrap();

                if actual_wat != expected_wat {
                    for diff in diff::lines(&expected_wat, &actual_wat) {
                        match diff {
                            diff::Result::Left(l) => println!("-{}", l),
                            diff::Result::Both(l, _) => println!(" {}", l),
                            diff::Result::Right(r) => println!("+{}", r),
                        }
                    }
                    panic!()
                }
            }
        };
    }

    test_export_global! {
        name = simple;
        input = r#"
		(module
			(global (;0;) (mut i32) (i32.const 1))
			(global (;1;) (mut i32) (i32.const 0)))
		"#;
        expected = r#"
		(module
			(global (;0;) (mut i32) (i32.const 1))
			(global (;1;) (mut i32) (i32.const 0))
			(export "exported_internal_global_0" (global 0))
			(export "exported_internal_global_1" (global 1)))
		"#
    }

    test_export_global! {
        name = with_import;
        input = r#"
		(module
			(import "env" "global" (global $global i64))
			(global (;0;) (mut i32) (i32.const 1))
			(global (;1;) (mut i32) (i32.const 0)))
		"#;
        expected = r#"
		(module
			(import "env" "global" (global $global i64))
			(global (;0;) (mut i32) (i32.const 1))
			(global (;1;) (mut i32) (i32.const 0))
			(export "exported_internal_global_0" (global 1))
			(export "exported_internal_global_1" (global 2)))
		"#
    }

    test_export_global! {
        name = with_import_and_some_are_immutable;
        input = r#"
		(module
			(import "env" "global" (global $global i64))
			(global (;0;) i32 (i32.const 1))
			(global (;1;) (mut i32) (i32.const 0)))
		"#;
        expected = r#"
		(module
			(import "env" "global" (global $global i64))
			(global (;0;) i32 (i32.const 1))
			(global (;1;) (mut i32) (i32.const 0))
			(export "exported_internal_global_0" (global 2)))
		"#
    }
}
