// Copyright 2019-2022 Parity Technologies (UK) Ltd.
// This file is dual-licensed as Apache-2.0 or GPL-3.0.
// see LICENSE for license details.

use crate::CratePath;
use darling::ToTokens as _;
use std::collections::HashMap;
use syn::parse_quote;

#[derive(Debug)]
pub struct TypeSubstitutes {
    pub(crate) inner: HashMap<String, syn::TypePath>,
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
        }
    }

    pub fn extend(
        &mut self,
        elems: impl IntoIterator<Item = (syn::TypePath, AbsoluteTypePath)>,
    ) {
        self.inner
            .extend(elems.into_iter().map(|(ty, AbsoluteTypePath(with))| {
                (
                    // TODO(xanewok): Take a special care around generics, qualified path etc.
                    ty.into_token_stream().to_string().replace(' ', ""),
                    with,
                )
            }));
    }
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
