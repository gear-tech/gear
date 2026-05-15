use clap::Parser;
use std::{fs, ops::Range, path::PathBuf};
use syn::{
    Expr, ExprCall, ExprMethodCall, File, ImplItem, ItemImpl, Stmt, spanned::Spanned, visit::Visit,
};

extern crate proc_macro2;

// Adjust specific weights by 10 percent
const WEIGHT_SLOPE_MULTI: (usize, usize) = (110, 100);
// The pattern to match function names
const WEIGHT_PATTERN: &str = "instr_";
// Descriptive comment
const WEIGHT_COMMENT: &str = "/* Adjust slope weight by 10 percent (Patched by script) */";

struct FunctionFinder<'a> {
    pattern: &'a str,
    patch_position: Vec<PatchRule>,
}

enum PatchRule {
    WeightFile {
        range: Range<usize>,
        add_parenthesis: bool,
    },
}

impl<'a> Visit<'_> for FunctionFinder<'a> {
    fn visit_item_impl(&mut self, i: &ItemImpl) {
        for item in &i.items {
            if let ImplItem::Fn(method) = item {
                let method_name = method.sig.ident.to_string();
                if method_name.starts_with(self.pattern) {
                    // Find the patch position
                    if let Some(patch_rule) =
                        find_patch_position(method.block.stmts.first().expect("first block in fn"))
                    {
                        self.patch_position.push(patch_rule);
                    }
                }
            }
        }
    }
}

fn find_patch_position(stmt: &Stmt) -> Option<PatchRule> {
    let Stmt::Expr(expr, ..) = stmt else {
        return None;
    };

    let Expr::MethodCall(ExprMethodCall { method, args, .. }) = expr else {
        return None;
    };

    if *method != "saturating_add" {
        return None;
    }
    let Some(Expr::MethodCall(ExprMethodCall {
        receiver, method, ..
    })) = args.first()
    else {
        return None;
    };

    if *method != "saturating_mul" {
        return None;
    }
    let Expr::Call(ExprCall { args, .. }) = receiver.as_ref() else {
        return None;
    };

    args.first().map(|arg| {
        let span = arg.span();
        let range = span.byte_range();
        PatchRule::WeightFile {
            range,
            add_parenthesis: !matches!(arg, Expr::Lit(_)),
        }
    })
}

fn patch_entries(content: &str, patch_positions: Vec<PatchRule>) -> String {
    let patch_positions = patch_positions.iter().rev().collect::<Vec<_>>();
    let mut patched_content = content.as_bytes().to_vec();

    for range in patch_positions {
        let PatchRule::WeightFile {
            range,
            add_parenthesis,
        } = range;

        let target = &patched_content[range.start..range.end];

        let expr = std::str::from_utf8(target).unwrap();
        let patched = if *add_parenthesis {
            format!(
                "({}) * {} / {} {WEIGHT_COMMENT}",
                expr, WEIGHT_SLOPE_MULTI.0, WEIGHT_SLOPE_MULTI.1
            )
        } else {
            format!(
                "{} * {} / {} {WEIGHT_COMMENT}",
                expr, WEIGHT_SLOPE_MULTI.0, WEIGHT_SLOPE_MULTI.1
            )
        }
        .as_bytes()
        .to_vec();

        let _ = patched_content.splice(range.clone(), patched);
    }

    String::from_utf8(patched_content).unwrap()
}

#[derive(Debug, Parser)]
struct Cli {
    #[clap(short)]
    input: PathBuf,
    #[clap(short)]
    output: PathBuf,
}

fn main() {
    let args = Cli::parse();

    // Read the file content
    let content = fs::read_to_string(args.input).expect("Unable to read file");

    // Parse the file content into a syntax tree
    let syntax_tree: File = syn::parse_file(&content).expect("Unable to parse file");

    // Create a FunctionFinder and visit the syntax tree
    let mut finder = FunctionFinder {
        pattern: WEIGHT_PATTERN,
        patch_position: vec![],
    };
    syn::visit::visit_file(&mut finder, &syntax_tree);

    // Patch the entries
    let patched = patch_entries(&content, finder.patch_position);
    std::fs::write(args.output, patched).expect("Unable to write file");
}
