// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.
use proc_macro2::Span;
use syn::{
    parse_quote,
    punctuated::Punctuated,
    token::{Comma, Plus},
    Expr, Generics, Ident, PathArguments, PathSegment, Token, TraitBound, TraitBoundModifier,
    TypeParam, TypeParamBound,
};

/// Describes how to output documentation for `_for_reply_(as)`
pub enum DocumentationStyle {
    /// `self::{func_name}`
    Function,
    /// `Self::{func_name}`
    Method,
}

impl DocumentationStyle {
    pub fn output(&self, func_name: &str) -> String {
        match self {
            DocumentationStyle::Function => format!("self::{func_name}"),
            DocumentationStyle::Method => format!("Self::{func_name}"),
        }
    }
}

const SPAN_CODEC: &str = "${CODEC}";
const SPAN_ELSE: &str = "${ELSE}";
const SPAN_ELSE_HREF: &str = "${ELSE_HREF}";
const SPAN_IDENT: &str = "${IDENT}";
const SPAN_IDENT_HREF: &str = "${IDENT_HREF}";
const WAIT_FOR_REPLY_DOCS_TEMPLATE: &str = r#"
 Same as [`${IDENT}`](${IDENT_HREF}), but the program
 will interrupt until the reply is received. ${CODEC}

 # See also

 - [`${ELSE}`](${ELSE_HREF})
"#;

/// New `Ident`
pub fn ident(s: &str) -> Ident {
    Ident::new(s, Span::call_site())
}

/// Appends suffix to ident
pub fn with_suffix(i: &Ident, suffix: &str) -> Ident {
    ident(&format!("{i}{suffix}"))
}

/// Get arguments from the inputs for function signature
pub fn get_args(inputs: &Punctuated<syn::FnArg, syn::token::Comma>) -> Expr {
    let idents = inputs.iter().filter_map(|param| {
        if let syn::FnArg::Typed(pat_type) = param {
            if let syn::Pat::Ident(pat_ident) = *pat_type.pat.clone() {
                return Some(pat_ident.ident);
            }
        }
        None
    });

    let mut punctuated: Punctuated<syn::Ident, Comma> = Punctuated::new();
    idents.for_each(|ident| punctuated.push(ident));

    parse_quote!(( #punctuated ))
}

/// Parse `dyn AsRef<str>` to `Expr`
pub fn wait_for_reply_docs(name: String, style: DocumentationStyle) -> (String, String) {
    let href = style.output(&name);
    let docs = WAIT_FOR_REPLY_DOCS_TEMPLATE
        .trim_start_matches('\n')
        .replace(SPAN_IDENT, &name)
        .replace(SPAN_IDENT_HREF, &href);

    let for_reply = format!("{name}_for_reply");
    let for_reply_href = style.output(&for_reply);

    let for_reply_as = format!("{name}_for_reply_as");
    let for_reply_as_href = style.output(&for_reply_as);

    (
        docs.replace(SPAN_ELSE, &for_reply_as)
            .replace(SPAN_ELSE_HREF, &for_reply_as_href)
            .replace(SPAN_CODEC, ""),
        docs.replace(SPAN_ELSE, &for_reply)
            .replace(SPAN_ELSE_HREF, &for_reply_href)
            .replace(
                SPAN_CODEC,
                "\n\n The output should be decodable via SCALE codec.",
            )
            + " - <https://docs.substrate.io/reference/scale-codec>",
    )
}

/// Append new generic to `Generics`
///
/// # Note
///
/// Only supports `TraitBound` for now.
pub fn append_generic(mut generics: Generics, ident: Ident, traits: Vec<Ident>) -> Generics {
    let mut bounds: Punctuated<TypeParamBound, Plus> = Punctuated::new();
    for t in traits {
        bounds.push_value(
            TraitBound {
                paren_token: None,
                modifier: TraitBoundModifier::None,
                lifetimes: None,
                path: PathSegment {
                    ident: t,
                    arguments: PathArguments::None,
                }
                .into(),
            }
            .into(),
        )
    }

    // push `Comma` if there is any generic params
    if !generics.params.is_empty() {
        generics.params.push_punct(Token![,](Span::call_site()));
    }

    generics.params.push_value(
        TypeParam {
            attrs: Default::default(),
            ident,
            colon_token: Some(Token![:](Span::call_site())),
            bounds,
            eq_token: None,
            default: None,
        }
        .into(),
    );

    generics
}
