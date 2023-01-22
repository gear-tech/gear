use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use std::{borrow::Borrow, fmt::Display, iter};
use syn::{
    parse_macro_input, spanned::Spanned, Attribute, Error, FnArg, Item, ItemMod, Pat, PatIdent,
    ReturnType, Type, TypePath, Visibility,
};

fn error(spanned: impl Spanned, message: impl Display) -> TokenStream {
    Error::new(spanned.span(), message)
        .into_compile_error()
        .into()
}

macro_rules! error {
    ($spanned:expr, $message:expr) => {
        return error($spanned, $message)
    };
}

macro_rules! if_some_error {
    ($option:expr, $message:expr) => {
        if let Some(spanned) = $option {
            error!(spanned, $message);
        }
    };
}

fn is_public(spanned: impl Spanned, visibility: &Visibility) -> Result<(), TokenStream> {
    match visibility {
        Visibility::Public(_) => Ok(()),
        other => Err(match other {
            Visibility::Inherited => error(spanned, "must be public, add the `pub` keyword"),
            _ => error(other, "mustn't be restricted, use the `pub` keyword alone"),
        }),
    }
}

fn has_attributes(
    attributes: impl IntoIterator<Item = impl Borrow<Attribute>>,
    message: impl Display,
) -> Result<(), TokenStream> {
    let mut attributes = attributes.into_iter();

    if let Some(attribute) = attributes.next() {
        let span = attributes.fold(attribute.borrow().span(), |span, attribute| {
            span.join(attribute.borrow().span()).unwrap_or(span)
        });

        Err(error(span, message))
    } else {
        Ok(())
    }
}

#[proc_macro_attribute]
pub fn metawasm(_: TokenStream, item: TokenStream) -> TokenStream {
    let module: ItemMod = parse_macro_input!(item);
    let module_span = module.span();

    if let Err(error) = has_attributes(
        module.attrs,
        "module with #[metawasm] mustn't have attributes",
    ) {
        return error;
    }

    let (potential_type_item, potential_functions) = if let Some((_, items)) = module.content {
        if items.is_empty() {
            return Default::default();
        }

        let mut items = items.into_iter();
        let two_first_items = (items.next(), items.next());

        if let (Some(first), Some(second)) = two_first_items {
            (first, iter::once(second).chain(items))
        } else {
            error!(
                module_span,
                "module with #[metawasm] must contain at least 2 items"
            );
        }
    } else {
        error!(
            module_span,
            "`#[metawasm]` doesn't work with modules without a body"
        );
    };

    // Checking the `State` type

    let type_item = if let Item::Type(type_item) = potential_type_item {
        type_item
    } else {
        error!(
            potential_type_item,
            "first item of a module with `#[metawasm]` must be a type alias to a state type (e.g. `type State = StateType;`)"
        );
    };
    let type_item_attributes = &type_item.attrs;

    let (state_type, state_type_inner) = if type_item.ident == "State" {
        if let Err(error) = is_public(&type_item, &type_item.vis) {
            return error;
        }

        if type_item.generics.params.is_empty() {
            (
                TypePath {
                    qself: None,
                    path: type_item.ident.into(),
                }
                .into(),
                *type_item.ty,
            )
        } else {
            error!(type_item.generics, "must be without generics");
        }
    } else {
        error!(
            type_item.ident,
            "identifier of the state type must be `State`"
        );
    };

    // Checking functions

    let mut functions = vec![];

    for potential_function in potential_functions {
        let function = if let Item::Fn(function) = potential_function {
            function
        } else {
            error!(
                potential_function,
                "rest of items in a module with `#[metawasm]` must be functions"
            );
        };

        if let Err(error) = is_public(&function, &function.vis) {
            return error;
        }

        let signature = function.sig;

        if_some_error!(signature.constness, "mustn't be constant");
        if_some_error!(signature.asyncness, "mustn't be asynchronous");
        if_some_error!(signature.unsafety, "mustn't be unsafe");
        if_some_error!(signature.abi, "mustn't have a binary interface");
        if_some_error!(signature.variadic, "mustn't have the variadic argument");

        if !signature.generics.params.is_empty() {
            error!(signature.generics, "mustn't have generics");
        }

        if signature.inputs.len() > 19 {
            error!(signature.inputs, "too many arguments, no more 19 arguments must be here due restrictions of the SCALE codec");
        }

        let signature_span = signature.span();
        let mut inputs = signature.inputs.into_iter();

        // Retrieving the first argument

        let first = if let Some(first) = inputs.next() {
            if let FnArg::Typed(first) = first {
                first
            } else {
                error!(first, "first argument must be `state: State`");
            }
        } else {
            error!(
                signature.paren_token.span,
                "mustn't be empty, add `state: State`"
            );
        };

        // Checking the first argument's name

        let first_ident = if let Pat::Ident(first) = *first.pat {
            if let Err(error) = has_attributes(&first.attrs, "mustn't have attributes") {
                return error;
            }

            if_some_error!(first.by_ref, "mustn't be bound to a reference");

            if let Some((at, subput)) = first.subpat {
                error!(
                    subput.span().join(at.span).unwrap_or(at.span),
                    "mustn't have a subpattern"
                );
            }

            if first.ident != "state" {
                error!(first.ident, "first argument's name must be `state`");
            }

            first
        } else {
            error!(first.pat, "unsupported pattern, use just `state`")
        };

        // Checking the first argument's type

        match *first.ty {
            Type::Reference(reference) => {
                if *reference.elem == state_type {
                    let lifetime_span = reference.lifetime.map(|lifetime| lifetime.span());
                    let mutability_span = reference.mutability.map(|mutability| mutability.span());

                    let lifetime_mutability = lifetime_span
                        .and_then(|lifetime_span| {
                            mutability_span
                                .and_then(|mutability_span| mutability_span.join(lifetime_span))
                                .or(Some(lifetime_span))
                        })
                        .or(mutability_span);

                    let span = lifetime_mutability
                        .and_then(|lifetime_mutability| {
                            lifetime_mutability.join(reference.and_token.span)
                        })
                        .unwrap_or(reference.and_token.span);

                    error!(span, "mustn't take a reference");
                }
            }
            first_type => {
                if first_type != state_type {
                    error!(first_type, "first argument's type must be `State`");
                }
            }
        }

        // Checking the rest of arguments

        let mut arguments = vec![];

        for argument in inputs {
            if let FnArg::Typed(argument) = argument {
                if let Pat::Ident(argument_ident) = *argument.pat {
                    if let Err(error) =
                        has_attributes(argument_ident.attrs.iter(), "mustn't have attributes")
                    {
                        return error;
                    }

                    if_some_error!(argument_ident.by_ref, "mustn't be bound to a reference");

                    if let Some((at, subput)) = argument_ident.subpat {
                        error!(
                            subput.span().join(at.span).unwrap_or(at.span),
                            "mustn't have a subpattern"
                        );
                    }

                    arguments.push((argument_ident, argument.ty));
                } else {
                    error!(argument.pat, "unsupported pattern, use just an identifier")
                }
            } else {
                // The rest of arguments can't be the `self` argument because
                // the compiler won't allow this.
                unreachable!("unexpected `self` argument");
            }
        }

        // Checking an output

        let return_type = match signature.output {
            ReturnType::Default => error!(signature_span, "return type must be specified"),
            ReturnType::Type(_, return_type) => {
                if *return_type == state_type || *return_type == state_type_inner {
                    error!(
                        return_type,
                        "mustn't have the same return type as the `State` type alias"
                    )
                }

                if let Type::Tuple(tuple) = *return_type {
                    error!(tuple, "return type mustn't be `()`");
                }

                return_type
            }
        };

        functions.push((
            function.attrs,
            signature.ident,
            first_ident,
            arguments,
            return_type,
            function.block,
        ));
    }

    // Code generating

    let mut type_registrations = Vec::with_capacity(functions.len());
    let (mut extern_functions, mut public_functions) =
        (type_registrations.clone(), type_registrations.clone());

    for (attributes, function_identifier, state_identifier, arguments, return_type, block) in
        functions
    {
        let (input_type, (variables, variables_types, variables_wo_parentheses), arguments) =
            process_arguments(arguments, state_identifier);

        let stringed_funident = function_identifier.to_string();
        let output = register_type(&return_type);

        type_registrations.push(quote! {
            funcs.insert(#stringed_funident.into(), ::gmeta::TypesRepr { input: #input_type, output: #output });
        });

        extern_functions.push(quote! {
            #[no_mangle]
            extern "C" fn #function_identifier() {
                let #variables: #variables_types = ::gstd::msg::load()
                    .expect("failed to load or decode a payload");

                ::gstd::msg::reply(super::#function_identifier(#variables_wo_parentheses), 0)
                    .expect("failed to encode or reply with a result from a metawasm function");
            }
        });

        public_functions.push(quote! {
            #(#attributes)*
            pub fn #function_identifier(#arguments) -> #return_type #block
        });
    }

    quote! {
        mod r#extern {
            use super::*;

            #[no_mangle]
            extern "C" fn metadata() {
                let mut funcs = ::gstd::BTreeMap::new();
                let mut registry = ::gmeta::Registry::new();

                #(#type_registrations)*

                let metawasm_data = ::gmeta::MetawasmData {
                    funcs,
                    registry: ::gstd::Encode::encode(&::gmeta::PortableRegistry::from(registry)),
                };

                ::gstd::msg::reply(metawasm_data, 0).expect("failed to encode or reply with metawasm data");
            }

            #(#extern_functions)*
        }

        #(#type_item_attributes)*
        pub type #state_type = #state_type_inner;

        #(#public_functions)*
    }.into()
}

fn process_arguments(
    arguments: Vec<(PatIdent, Box<Type>)>,
    state_identifier: PatIdent,
) -> (
    proc_macro2::TokenStream,
    (
        proc_macro2::TokenStream,
        proc_macro2::TokenStream,
        proc_macro2::TokenStream,
    ),
    proc_macro2::TokenStream,
) {
    if arguments.is_empty() {
        let variables = quote!(state);

        (
            quote!(None),
            (variables.clone(), quote!(super::State), variables),
            quote!(#state_identifier: State),
        )
    } else {
        let arguments_idents = arguments.iter().map(|argument| &argument.0.ident);
        let variables_wo_parentheses = quote!(#(#arguments_idents),*);

        let (variables, variables_types) = {
            let arguments_types = arguments.iter().map(|argument| argument.1.as_ref());
            let variables_types_wo_parentheses = quote!(#(#arguments_types),*);

            if arguments.len() > 1 {
                (
                    quote!((#variables_wo_parentheses)),
                    quote!((#variables_types_wo_parentheses)),
                )
            } else {
                (
                    variables_wo_parentheses.clone(),
                    variables_types_wo_parentheses,
                )
            }
        };

        let input_type = register_type(variables_types.clone());

        let arguments = arguments.into_iter().map(|(name, ty)| quote!(#name: #ty));

        (
            input_type,
            (
                quote!((#variables, state)),
                quote!((#variables_types, super::State)),
                quote!(state, #variables_wo_parentheses),
            ),
            quote!(#state_identifier: State, #(#arguments),*),
        )
    }
}

fn register_type(ty: impl ToTokens) -> proc_macro2::TokenStream {
    let ty = ty.to_token_stream();

    quote! {
        Some(registry.register_type(&::gmeta::MetaType::new::<#ty>()).id())
    }
}
