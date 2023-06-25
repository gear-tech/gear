use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use std::{borrow::Borrow, fmt::Display, iter};
use syn::{
    parse_macro_input, spanned::Spanned, Attribute, Error, FnArg, Item, ItemMod, Pat, ReturnType,
    Type, TypePath, Visibility,
};

static MODULE_NAME: &str = "metafns";

fn error<T>(spanned: impl Spanned, message: impl Display) -> Result<T, Error> {
    Err(Error::new(spanned.span(), message))
}

macro_rules! return_error_if_some {
    ($option:expr, $message:expr) => {
        if let Some(spanned) = $option {
            return error(spanned, $message);
        }
    };
}

fn validate_if_private(spanned: impl Spanned, visibility: &Visibility) -> Result<(), Error> {
    match visibility {
        Visibility::Public(_) => Ok(()),
        other => match other {
            Visibility::Inherited => {
                error(spanned, "visibility must be public, add the `pub` keyword")
            }
            _ => error(
                other,
                "visibility mustn't be restricted, use the `pub` keyword alone",
            ),
        },
    }
}

fn validate_if_has_no_attributes(
    attributes: impl IntoIterator<Item = impl Borrow<Attribute>>,
    message: impl Display,
) -> Result<(), Error> {
    let mut attributes = attributes.into_iter();

    if let Some(attribute) = attributes.next() {
        let span = attributes.fold(attribute.borrow().span(), |span, attribute| {
            span.join(attribute.borrow().span()).unwrap_or(span)
        });

        error(span, message)
    } else {
        Ok(())
    }
}

/// Generates metawasm functions.
///
/// An example of the expected structure:
/// ```
/// use gstd::prelude::*;
///
/// #[derive(Decode, Encode, TypeInfo)]
/// pub struct StateType;
///
/// #[derive(Encode, TypeInfo)]
/// pub struct SomeReturnType;
///
/// #[derive(Decode, TypeInfo)]
/// pub struct SomeArg;
///
/// #[gmeta::metawasm]
/// pub mod metafns {
///     pub type State = StateType;
///
///     /// Documentation...
///     pub fn some_function(_: State) -> SomeReturnType {
///         unimplemented!()
///     }
///
///     pub fn another_function_but_with_arg(mut _state: State, _arg: SomeArg) -> State {
///         unimplemented!()
///     }
///
///     /// Another doc...
///     pub fn function_with_multiple_args(
///         _state: State,
///         mut _arg1: SomeArg,
///         _arg2: u16,
///         mut _arg3: u32,
///     ) -> SomeReturnType {
///         unimplemented!()
///     }
/// }
/// # fn main() {}
/// ```
///
/// # Syntax
///
/// - This attribute **must** be used on the `pub`lic `mod` container with the
/// `metafns` identifier.
/// - The first item in the module **must** be a `pub`lic `type` alias with the
/// `State` identifier. The type for which `State` will be an alias **must**
/// implement [`Decode`] trait.
///
/// Usually the state type should be imported from the implemented associative
/// [`Metadata::State`](../gmeta/trait.Metadata.html#associatedtype.State) type
/// from the contract's `io` crate.
///
/// - The rest of items **must** be `pub`lic functions.
/// - The first argument's type of metafunctions **must** be `State`.
/// - If the first argument uses
/// [the identifier pattern](https://doc.rust-lang.org/stable/reference/patterns.html#identifier-patterns),
/// the identifier **must** be `state` or `_state`.
///
/// In addition to the mandatory first argument, functions can have additional
/// ones.
///
/// - The maximum amount of additional arguments is 18 due restrictions of the
/// SCALE codec.
/// - All additional arguments **must** implement the [`Decode`] &
/// [`TypeInfo`] traits.
/// - A function **mustn't** return `()` or nothing.
/// - A returned type **must** implement the
/// [`Encode`](../gmeta/trait.Encode.html) & [`TypeInfo`] traits.
///
/// [`Decode`]: ../gmeta/trait.Decode.html
/// [`TypeInfo`]: ../gmeta/trait.TypeInfo.html
///
/// # Expansion result
///
/// This attribute doesn't change the `metafns` module and items inside, but
/// adds `use super::*;` inside the module because, in most cases, it'll be
/// useful for importing items from an upper namespace. So every item in the
/// same namespace where the module is located is accessible inside it.
///
/// The rest of the magic happens in the another generated private `extern`
/// module. It registers all metawasm functions, their arguments & return types,
/// and generates extern functions with the same names. Later, they can be
/// called from a metaWASM binary inside a blockchain.
///
/// **Important note**: although metafunctions can take more than 1 additional
/// arguments, on the metaWASM binary level, they must be passed as one. So if
/// the amount of additinal arguments is 0 or 1, nothing needs to be changed,
/// but if more - they all must be placed inside a tuple in the same order as in
/// their function's signature.
///
/// E.g., argument definitions for the above example:
/// - For `some_function` an argument must be [`None`].
/// - For `another_function_but_with_arg` an argument must be `Some(SomeArg)`.
/// - For `function_with_multiple_args` an argument must be
/// `Some((SomeArg, u16, u32))`.
#[proc_macro_attribute]
pub fn metawasm(_: TokenStream, item: TokenStream) -> TokenStream {
    process(parse_macro_input!(item)).unwrap_or_else(|error| error.into_compile_error().into())
}

fn process(module: ItemMod) -> Result<TokenStream, Error> {
    let module_span = module.span();

    validate_if_has_no_attributes(
        module.attrs,
        "module with #[metawasm] mustn't have attributes",
    )?;
    validate_if_private(module_span, &module.vis)?;

    if module.ident != MODULE_NAME {
        return error(
            module.ident,
            format_args!("name of a module with #[metawasm] must be `{MODULE_NAME}`"),
        );
    }

    let Some((_, items)) = module.content else {
        return error(
            module_span,
            "`#[metawasm]` doesn't work with modules without a body"
        );
    };

    if items.is_empty() {
        return Ok(Default::default());
    }

    let mut items = items.into_iter();
    let two_first_items = (items.next(), items.next());

    let (potential_type_item, potential_functions) =
        if let (Some(first), Some(second)) = two_first_items {
            (first, iter::once(second).chain(items))
        } else {
            return error(
                module_span,
                "module with #[metawasm] must contain the `State` type alias & at least 1 function",
            );
        };

    // Checking the `State` type

    let Item::Type(type_item) = potential_type_item else {
        return error(
            potential_type_item,
            "first item of a module with `#[metawasm]` must be a type alias to a state type (e.g. `type State = StateType;`)"
        );
    };
    let type_item_attributes = &type_item.attrs;

    let (state_type, state_type_inner) = if type_item.ident == "State" {
        validate_if_private(&type_item, &type_item.vis)?;

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
            return error(type_item.generics, "must be without generics");
        }
    } else {
        return error(
            type_item.ident,
            "identifier of the state type must be `State`",
        );
    };

    // Checking functions

    let mut functions = vec![];

    for potential_function in potential_functions {
        let Item::Fn(function) = potential_function else {
            return error(
                potential_function,
                "rest of items in a module with `#[metawasm]` must be functions"
            );
        };

        validate_if_private(&function, &function.vis)?;

        let signature = function.sig;

        return_error_if_some!(signature.constness, "mustn't be constant");
        return_error_if_some!(signature.asyncness, "mustn't be asynchronous");
        return_error_if_some!(signature.unsafety, "mustn't be unsafe");
        return_error_if_some!(signature.abi, "mustn't have a binary interface");
        return_error_if_some!(signature.variadic, "mustn't have the variadic argument");

        if !signature.generics.params.is_empty() {
            return error(signature.generics, "mustn't have generics");
        }

        if signature.inputs.len() > 19 {
            return error(signature.inputs, "too many arguments, no more 19 arguments must be here due restrictions of the SCALE codec");
        }

        let signature_span = signature.span();
        let mut inputs = signature.inputs.into_iter();

        // Retrieving the first argument

        let first = if let Some(first) = inputs.next() {
            if let FnArg::Typed(first) = first {
                validate_if_has_no_attributes(&first.attrs, "mustn't have attributes")?;

                first
            } else {
                return error(first, "mustn't be `self`");
            }
        } else {
            return error(
                signature.paren_token.span,
                "mustn't be empty, add `state: State` or `_: State`",
            );
        };

        // Checking the first argument's name

        if let Pat::Ident(ref pat_ident) = *first.pat {
            if pat_ident.ident != "state" && pat_ident.ident != "_state" {
                return error(&pat_ident.ident, "must be `state` or `_state`");
            }
        }

        // Checking the first argument's type

        match *first.ty {
            Type::Reference(reference) if *reference.elem == state_type => {
                let lifetime_span = reference.lifetime.map(|lifetime| lifetime.span());
                let mutability_span = reference.mutability.map(|mutability| mutability.span());

                let lifetime_mutability = lifetime_span.map_or(mutability_span, |lifetime_span| {
                    Some(
                        mutability_span
                            .and_then(|mutability_span| mutability_span.join(lifetime_span))
                            .unwrap_or(lifetime_span),
                    )
                });

                let span = lifetime_mutability
                    .and_then(|lifetime_mutability| {
                        lifetime_mutability.join(reference.and_token.span)
                    })
                    .unwrap_or(reference.and_token.span);

                return error(span, "mustn't take a reference");
            }
            first_type => {
                if first_type != state_type {
                    return error(first_type, "first argument's type must be `State`");
                }
            }
        }

        // Checking the rest of arguments

        let mut arguments = vec![];

        for argument in inputs {
            if let FnArg::Typed(argument) = argument {
                validate_if_has_no_attributes(&argument.attrs, "mustn't have attributes")?;

                arguments.push((argument.pat, argument.ty));
            } else {
                // The rest of arguments can't be the `self` argument because
                // the compiler won't allow this.
                unreachable!("unexpected `self` argument");
            }
        }

        // Checking an output

        let return_type = match signature.output {
            ReturnType::Default => {
                return error(signature_span, "return type must be specified");
            }
            ReturnType::Type(_, return_type) => {
                if let Type::Tuple(ref tuple) = *return_type {
                    if tuple.elems.is_empty() {
                        return error(tuple, "return type mustn't be `()`");
                    }
                }

                return_type
            }
        };

        functions.push((
            function.attrs,
            signature.ident,
            first.pat,
            arguments,
            return_type,
            function.block,
        ));
    }

    // Code generating

    let mut type_registrations = Vec::with_capacity(functions.len());
    let (mut extern_functions, mut public_functions) =
        (type_registrations.clone(), type_registrations.clone());

    for (attributes, function_identifier, state_pattern, arguments, return_type, block) in functions
    {
        let CodeGenItems {
            input_type,
            variables,
            variables_types,
            variables_wo_parentheses,
            arguments,
        } = process_arguments(arguments, state_pattern);

        let stringed_fn_ident = function_identifier.to_string();
        let output = register_type(&return_type);

        type_registrations.push(quote! {
            funcs.insert(#stringed_fn_ident.into(), ::gmeta::TypesRepr { input: #input_type, output: #output });
        });

        extern_functions.push(quote! {
            #[no_mangle]
            extern "C" fn #function_identifier() {
                let #variables: #variables_types = ::gstd::msg::load()
                    .expect("Failed to load or decode a payload");

                ::gstd::msg::reply(super::#function_identifier(#variables_wo_parentheses), 0)
                    .expect("Failed to encode or reply with a result from a metawasm function");
            }
        });

        public_functions.push(quote! {
            #(#attributes)*
            pub fn #function_identifier(#arguments) -> #return_type #block
        });
    }

    let module_ident = quote::format_ident!("{MODULE_NAME}");

    Ok(quote! {
        pub mod #module_ident {
            use super::*;

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

                    ::gstd::msg::reply(metawasm_data, 0).expect("Failed to encode or reply with metawasm data");
                }

                #(#extern_functions)*
            }

            #(#type_item_attributes)*
            pub type #state_type = #state_type_inner;

            #(#public_functions)*
        }
    }.into())
}

struct CodeGenItems {
    input_type: proc_macro2::TokenStream,
    variables: proc_macro2::TokenStream,
    variables_types: proc_macro2::TokenStream,
    variables_wo_parentheses: proc_macro2::TokenStream,
    arguments: proc_macro2::TokenStream,
}

fn process_arguments(
    arguments: Vec<(Box<Pat>, Box<Type>)>,
    state_pattern: Box<Pat>,
) -> CodeGenItems {
    if arguments.is_empty() {
        let variables = quote!(state);

        CodeGenItems {
            input_type: quote!(None),
            variables: variables.clone(),
            variables_types: quote!(State),
            variables_wo_parentheses: variables,
            arguments: quote!(#state_pattern: State),
        }
    } else {
        let arguments_types = arguments.iter().map(|argument| &argument.1);
        let variables_types_wo_parentheses = quote!(#(#arguments_types),*);

        let (variables_wo_parentheses, variables, variables_types) = if arguments.len() > 1 {
            let variables_wo_parentheses =
                (0..arguments.len()).map(|index| quote::format_ident!("arg{}", index));
            let variables_wo_parentheses = quote!(#(#variables_wo_parentheses),*);

            let variables_with_parentheses = quote!((#variables_wo_parentheses));

            (
                variables_wo_parentheses,
                variables_with_parentheses,
                quote!((#variables_types_wo_parentheses)),
            )
        } else {
            let variables_wo_parentheses = quote!(arg);

            (
                variables_wo_parentheses.clone(),
                variables_wo_parentheses,
                variables_types_wo_parentheses,
            )
        };

        let input_type = register_type(variables_types.clone());

        let arguments = arguments
            .into_iter()
            .map(|(pattern, ty)| quote!(#pattern: #ty));

        CodeGenItems {
            input_type,
            variables: quote!((#variables, state)),
            variables_types: quote!((#variables_types, State)),
            variables_wo_parentheses: quote!(state, #variables_wo_parentheses),
            arguments: quote!(#state_pattern: State, #(#arguments),*),
        }
    }
}

fn register_type(ty: impl ToTokens) -> proc_macro2::TokenStream {
    let ty = ty.to_token_stream();

    quote! {
        Some(registry.register_type(&::gmeta::MetaType::new::<#ty>()).id)
    }
}
