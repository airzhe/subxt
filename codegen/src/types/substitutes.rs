// Copyright 2019-2022 Parity Technologies (UK) Ltd.
// This file is dual-licensed as Apache-2.0 or GPL-3.0.
// see LICENSE for license details.

use crate::CratePath;
use darling::ToTokens as _;
use proc_macro_error::{abort, emit_warning};
use std::collections::HashMap;
use syn::{parse_quote, spanned::Spanned as _};

use super::{TypePath, TypePathType};

#[derive(Debug)]
pub struct TypeSubstitutes {
    pub(crate) inner: HashMap<String, syn::TypePath>,
    params: HashMap<String, Vec<TypePath>>,
}

impl TypeSubstitutes {
    pub fn new(crate_path: &CratePath) -> Self {
        // Some hardcoded default type substitutes, can be overridden by user
        let defaults = [
            (
                "bitvec::order::Lsb0",
                parse_quote!(#crate_path::utils::bits::Lsb0),
            ),
            (
                "bitvec::order::Msb0",
                parse_quote!(#crate_path::utils::bits::Msb0),
            ),
            (
                "sp_core::crypto::AccountId32",
                parse_quote!(#crate_path::ext::sp_core::crypto::AccountId32),
            ),
            (
                "primitive_types::H160",
                parse_quote!(#crate_path::ext::sp_core::H160),
            ),
            (
                "primitive_types::H256",
                parse_quote!(#crate_path::ext::sp_core::H256),
            ),
            (
                "primitive_types::H512",
                parse_quote!(#crate_path::ext::sp_core::H512),
            ),
            (
                "sp_runtime::multiaddress::MultiAddress",
                parse_quote!(#crate_path::ext::sp_runtime::MultiAddress),
            ),
            (
                "frame_support::traits::misc::WrapperKeepOpaque",
                parse_quote!(#crate_path::utils::WrapperKeepOpaque),
            ),
            // BTreeMap and BTreeSet impose an `Ord` constraint on their key types. This
            // can cause an issue with generated code that doesn't impl `Ord` by default.
            // Decoding them to Vec by default (KeyedVec is just an alias for Vec with
            // suitable type params) avoids these issues.
            ("BTreeMap", parse_quote!(#crate_path::utils::KeyedVec)),
            ("BTreeSet", parse_quote!(::std::vec::Vec)),
        ];

        Self {
            inner: defaults
                .into_iter()
                .map(|(path, substitute)| (path.to_owned(), substitute))
                .collect(),
            params: Default::default(),
        }
    }

    pub fn extend(
        &mut self,
        elems: impl IntoIterator<Item = (syn::TypePath, AbsoluteTypePath)>,
    ) {
        self.inner
            .extend(elems.into_iter().map(|(ty, AbsoluteTypePath(with))| {
                // TODO: Verify both paths
                let src_namespace = || ty.path.segments.iter().rev().skip(1);
                if let Some(seg) = src_namespace()
                    .find(|seg| !seg.arguments.is_none() && !seg.arguments.is_empty())
                {
                    abort!(seg.arguments.span(), "Namespace segment can't be generic");
                }
                let Some(syn::PathSegment { arguments: src_path_args, ..}) = ty.path.segments.last() else { abort!(ty.span(), "Empty path") };
                let Some(syn::PathSegment { arguments: target_path_args, ..}) = with.path.segments.last() else { abort!(ty.span(), "Empty path") };

                let source_args: Vec<_> = type_args(src_path_args).collect();
                // Generics were specified in the source type, so we substitute
                // them
                if !source_args.is_empty() {
                    let new_params = type_args(target_path_args).map(|arg| {
                        // TODO: Handle nested generics in a substituted path
                        if let Some(&src) = source_args.iter().find(|&src| src == &arg) {
                            // TODO: This surely wrongly interacts with unused generics etc.
                            TypePath::Type(TypePathType::Path { path: src.clone(), params: Vec::new()})
                         }
                        else if is_absolute(arg) {
                            TypePath::Type(TypePathType::Path { path: arg.clone(), params: Vec::new()})
                        } else {
                            abort!(arg.span(), "Generic parameter {} couldn't be found or not absolute")
                        }
                    }).collect();

                    self.params.insert(ty.to_token_stream().to_string().replace(' ', ""), new_params);
                }

                (
                    // TODO(xanewok): Take a special care around generics, qualified path etc.
                    ty.to_token_stream().to_string().replace(' ', ""),
                    with,
                )
            }));
    }

    /// Given a source type path and the (already resolved? this can't be right)
    /// type parameters, return a new path and optionally overwritten type parameters
    pub fn for_path_with_params<'a: 'b, 'b>(
        &'a self,
        path: &syn::TypePath,
        params: &'b [TypePath],
    ) -> Option<(&'a syn::TypePath, &'b [TypePath])> {
        // We only support:
        // 1. Reordering the generics
        // 2. Replacing the generic type with a concrete type (won't this affect parent_type_params logic?)
        // TypePath::Type(TypePathType::Path { path: todo!(), params: Vec::new()})
        // 3. Omitting certain generics

        let path_key = path.to_token_stream().to_string().replace(' ', "");
        if let Some(sub) = self.inner.get(&path_key) {
            let params = self
                .params
                .get(&path_key)
                .map(Vec::as_slice)
                .unwrap_or(params);

            return Some((sub, params));
        } else {
            return None;
        }
    }
}

/// Returns an iterator over generic type parameters for `syn::PathArguments`.
/// For example:
/// - `<'a, T>` should only return T
/// - `(A, B) -> String` shouldn't return anything
fn type_args(path_args: &syn::PathArguments) -> impl Iterator<Item = &syn::TypePath> {
    let args_opt = match path_args {
        syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
            ref args,
            ..
        }) => Some(args),
        _ => None,
    };

    args_opt
        .into_iter()
        .flat_map(|x| x)
        .filter_map(|arg| match arg {
            syn::GenericArgument::Type(syn::Type::Path(path)) => Some(path),
            _ => None,
        })
}

fn is_absolute(value: &syn::TypePath) -> bool {
    value.path.leading_colon.is_some()
        || value
            .path
            .segments
            .first()
            .map_or(false, |segment| segment.ident == "crate")
}

pub struct AbsoluteTypePath(syn::TypePath);

impl TryFrom<syn::TypePath> for AbsoluteTypePath {
    type Error = (syn::TypePath, String);
    fn try_from(value: syn::TypePath) -> Result<Self, Self::Error> {
        let is_global_abs_path = value.path.leading_colon.is_some()
            || value
                .path
                .segments
                .first()
                .map_or(false, |segment| segment.ident == "crate");

        if is_global_abs_path {
            Ok(AbsoluteTypePath(value))
        } else {
            Err(
                (value, "The substitute path must be a global absolute path; try prefixing with `::` or `crate`".to_owned())
            )
        }
    }
}
