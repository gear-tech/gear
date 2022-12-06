extern crate proc_macro;
use core::fmt::Display;
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    spanned::Spanned, Attribute, AttributeArgs, Block, Error, FnArg, ItemFn, ItemTrait, LitStr,
    Pat, ReturnType, TraitItem, Visibility,
};

macro_rules! ensure {
    ( $cond:expr, $msg:expr ) => {{
        ensure!($cond, quote!(), $msg)
    }};

    ( $cond:expr, $token:expr, $msg:expr ) => {{
        if !$cond {
            return Err(compile_error($token, $msg));
        }
    }};
}

fn compile_error<T: ToTokens, U: Display>(tokens: T, msg: U) -> TokenStream {
    Error::new_spanned(tokens, msg).into_compile_error().into()
}

fn parse_state_ident(item: TraitItem) -> Result<syn::Type, TokenStream> {
    let type_item = match item {
        TraitItem::Type(trait_item_type) => trait_item_type,
        _ => {
            return Err(compile_error(
                item,
                "First item of a trait supposed to be `type State = TYPE;`",
            ))
        }
    };

    // Validating type attrs.
    ensure!(
        type_item.attrs.is_empty(),
        &type_item.attrs[0],
        "Type attrs should be empty"
    );

    // Validation not required for `type_token`.

    // Validating type ident.
    ensure!(
        type_item.ident == "State",
        type_item.ident,
        "Incorrect identifier: should be `State`"
    );

    // Validating generics.
    ensure!(
        type_item.generics.params.is_empty(),
        &type_item.generics.params[0],
        "State type shouldn't contain generics"
    );
    ensure!(
        type_item.generics.where_clause.is_none(),
        type_item.generics.where_clause,
        "State type shouldn't contain where clause"
    );

    // Validation not required for `colon_token`.

    // Validating bounds absence.
    ensure!(
        type_item.bounds.is_empty(),
        &type_item.bounds[0],
        "State type shouldn't contain bounds"
    );

    // Validating default type existence.
    ensure!(
        type_item.default.is_some(),
        "State type should be specified as default type value of a trait"
    );

    // Validation not required for `semi_token`.

    let state_type = type_item.default.expect("Checked above").1;

    Ok(state_type)
}

fn parse_fn_item(item: TraitItem, state: &syn::Type) -> Result<ItemFn, TokenStream> {
    // TODO: validate here.
    let mut trait_item_method = match item {
        TraitItem::Method(trait_item_method) => trait_item_method,
        _ => {
            return Err(compile_error(
                item,
                "Unsupported trait item: it should contain 1 type and fns",
            ))
        }
    };

    // Validating method attrs.
    ensure!(
        trait_item_method.attrs.is_empty(),
        &trait_item_method.attrs[0],
        "Functions should not contain attributes"
    );

    // Validating method signature.
    let signature = &mut trait_item_method.sig;
    ensure!(
        signature.constness.is_none(),
        &signature.constness,
        "Function shouldn't be const"
    );
    ensure!(
        signature.asyncness.is_none(),
        &signature.asyncness,
        "Function shouldn't be async"
    );
    ensure!(
        signature.unsafety.is_none(),
        &signature.unsafety,
        "Function should be safe"
    );
    ensure!(
        signature.abi.is_none(),
        &signature.abi,
        "Function shouldn't be FFI"
    );
    // Validation not required for `fn_token`.
    // Validation not required for `ident`.
    ensure!(
        signature.generics.params.is_empty(),
        &signature.generics.params[0],
        "Function shouldn't have generics"
    );
    ensure!(
        signature.generics.where_clause.is_none(),
        &signature.generics.where_clause,
        "Function shouldn't have where clause"
    );
    // Validation not required for `parent_token`.
    let validate_typed = |arg: &FnArg, state_name: bool| -> Result<(), TokenStream> {
        match arg {
            FnArg::Typed(pat_type) => {
                ensure!(
                    pat_type.attrs.is_empty(),
                    &pat_type.attrs[0],
                    "Arguments shouldn't have attributes"
                );

                ensure!(
                    matches!(&*pat_type.ty, syn::Type::Path(_)),
                    &*pat_type.ty,
                    "Illegal type specification: should be path-based"
                );

                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    ensure!(
                        pat_ident.attrs.is_empty(),
                        &pat_ident.attrs[0],
                        "Pattern idents shouldn't have attributes"
                    );
                    ensure!(
                        pat_ident.by_ref.is_none(),
                        &pat_ident.by_ref,
                        "Pattern idents shouldn't be thrown as ref"
                    );
                    ensure!(
                        pat_ident.subpat.is_none(),
                        &pat_ident.subpat.as_ref().expect("checked in cond").1,
                        "Pattern idents shouldn't be thrown using subpattern"
                    );

                    if (pat_ident.ident == "state") != state_name {
                        return Err(compile_error(
                            &pat_ident.ident,
                            if state_name {
                                "Illegal name: should be `state`"
                            } else {
                                "Illegal name: shouldn't be `state`"
                            },
                        ));
                    }
                } else {
                    return Err(compile_error(
                        pat_type,
                        "Illegal pattern type: use common `var: Type`",
                    ));
                }

                Ok(())
            }
            _ => Err(compile_error(arg, "Self arguments are restricted")),
        }
    };
    let mutate_and_validate_state =
        |arg: &mut FnArg, state: &syn::Type| -> Result<(), TokenStream> {
            validate_typed(arg, true)?;

            if let FnArg::Typed(pat_type) = arg {
                if let syn::Type::Path(type_path) = pat_type.ty.as_ref() {
                    // TODO: replace with normal comparison
                    if type_path.to_token_stream().to_string() != "Self :: State" {
                        return Err(compile_error(
                            type_path,
                            "Incorrect state type: should be `Self::State`",
                        ));
                    }
                } else {
                    unreachable!("Guaranteed by `validate_typed`");
                }

                *pat_type.ty.as_mut() = state.clone();
            } else {
                unreachable!("Guaranteed by `validate_typed`");
            }

            Ok(())
        };
    // TODO: args validation
    let inputs = &mut signature.inputs;
    match inputs.len() {
        0 => {
            return Err(compile_error(
                &inputs,
                "Function should contain from 1 to 2 args",
            ))
        }
        1 => mutate_and_validate_state(&mut inputs[0], state)?,
        2 => {
            validate_typed(&inputs[0], false)?;
            mutate_and_validate_state(&mut inputs[1], state)?;
        }
        _ => {
            return Err(compile_error(
                &inputs[2],
                "Function shouldn't contain more than 2 args",
            ))
        }
    }

    ensure!(
        signature.variadic.is_none(),
        &signature.variadic,
        "Signature shouldn't have variadic"
    );
    // Validation not required for `output`.

    // Validating realization.
    ensure!(
        trait_item_method.default.is_some(),
        &trait_item_method,
        "Function should contain body"
    );

    // Validation not required for `semi_token`.

    // Constructing fn_item.
    let fn_item = syn::ItemFn {
        attrs: Default::default(),
        vis: Visibility::Inherited,
        sig: trait_item_method.sig,
        block: Box::new(trait_item_method.default.expect("Checked above")),
    };

    Ok(fn_item)
}

// It's validated in `parse_trait`, that items contains at least 1 element.
fn parse_trait_items(mut items: Vec<TraitItem>) -> Result<Vec<ItemFn>, TokenStream> {
    let maybe_state_item = items.remove(0);

    let state_ident = parse_state_ident(maybe_state_item)?;

    let mut funcs = Vec::with_capacity(items.len());

    for maybe_fn_item in items {
        let func = parse_fn_item(maybe_fn_item, &state_ident)?;
        funcs.push(func);
    }

    Ok(funcs)
}

fn parse_trait(item: ItemTrait) -> Result<Vec<TraitItem>, TokenStream> {
    // Validating attributes absence.
    ensure!(
        item.attrs.is_empty(),
        &item.attrs[0],
        "Trait attributes should be empty"
    );

    // Validating public visibility.
    ensure!(
        matches!(item.vis, Visibility::Public(_)),
        item.vis,
        "Trait should be public"
    );

    // Validating safety.
    ensure!(
        item.unsafety.is_none(),
        item.unsafety,
        "Trait should be safe"
    );

    // Validating non-auto trait.
    ensure!(
        item.auto_token.is_none(),
        item.auto_token,
        "Trait shouldn't be auto"
    );

    // Validation not required for `trait_token`
    // Validation not required for `ident`

    // Validating generics absence.
    // It's free to skip `lt_token` and `gt_token`.
    ensure!(
        item.generics.params.is_empty(),
        &item.generics.params[0],
        "Trait shouldn't contain generics"
    );
    ensure!(
        item.generics.where_clause.is_none(),
        item.generics.where_clause,
        "Trait shouldn't contain where clause"
    );

    // Validation not required for `colon_token`

    // Validating supertraits absence.
    ensure!(
        item.supertraits.is_empty(),
        &item.supertraits[0],
        "Trait shouldn't contain supertraits (bounds)"
    );

    // Validation not required for `brace_token`

    // Validating amount of items.
    ensure!(
        !item.items.is_empty(),
        "Trait should contain at least one item inside"
    );

    Ok(item.items)
}

fn construct_abi(funcs: Vec<ItemFn>) -> TokenStream {
    let mut res = proc_macro2::TokenStream::new();
    for mut func in funcs {
        let span = func.span();
        let prev_inputs = func.sig.inputs.clone();
        let prev_ident = func.sig.ident;
        let new_ident = syn::Ident::new(&format!("_{prev_ident}"), prev_ident.span());
        func.sig.ident = new_ident.clone();

        func.to_tokens(&mut res);

        func.attrs = vec![Attribute {
            pound_token: syn::token::Pound(span),
            style: syn::AttrStyle::Outer,
            bracket_token: syn::token::Bracket(span),
            path: syn::Path {
                leading_colon: None,
                segments: Default::default(),
            },
            tokens: quote!(no_mangle),
        }];

        let mut sig = func.sig;
        sig.ident = prev_ident;
        sig.abi = Some(syn::Abi {
            extern_token: syn::token::Extern(span),
            name: Some(LitStr::new("C", span)),
        });
        sig.inputs = Default::default();
        sig.output = ReturnType::Default;
        func.sig = sig;

        let state_ident = if let Some(FnArg::Typed(pat_type)) = prev_inputs.last() {
            pat_type.ty.as_ref()
        } else {
            unreachable!("Checked on validation");
        };

        let token = if prev_inputs.len() == 1 {
            quote! {{
                let state: #state_ident = gstd::msg::load().expect("Failed to decode state");
                let res = #new_ident(state);
                gstd::msg::reply(res, 0).expect("Failed to share result");
            }}
        } else {
            let arg_ident = if let Some(FnArg::Typed(pat_type)) = prev_inputs.first() {
                pat_type.ty.as_ref()
            } else {
                unreachable!("Checked on validation");
            };

            quote! {{
                let (arg, state): (#arg_ident, #state_ident) = gstd::msg::load().expect("Failed to decode state");
                let res = #new_ident(arg, state);
                gstd::msg::reply(res, 0).expect("Failed to share result");
            }}
        };

        let block: Block = syn::parse(token.into()).expect("Unreachable");
        func.block = Box::new(block);

        func.to_tokens(&mut res);
    }

    res.into()
}

#[proc_macro_attribute]
pub fn metawasm(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(attr as AttributeArgs);
    let trait_obj = syn::parse_macro_input!(item as ItemTrait);

    let f = || -> Result<TokenStream, TokenStream> {
        ensure!(
            args.is_empty(),
            &args[0],
            "#[metawasm] attributes should be empty"
        );

        let trait_items = parse_trait(trait_obj)?;

        let funcs = parse_trait_items(trait_items)?;

        let res = construct_abi(funcs);

        Ok(res)
    };

    match f() {
        Ok(v) => v,
        Err(e) => e,
    }
}
