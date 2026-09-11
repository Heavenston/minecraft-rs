use std::iter::zip;

use proc_macro2::TokenStream;
use quote::{ format_ident, quote };
use syn::parse::Parse;

use crate::{EntryResource, PseudoStruct, PseudoStructFields, kw};

#[derive(Clone)]
pub struct Entry {
    pub is_default: bool,
    pub resource: EntryResource,
}

impl Parse for Entry {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let is_default = if input.peek(kw::default) { input.parse::<kw::default>()?; true } else { false };
        let resource = input.parse()?;
        Ok(Self {
            is_default,
            resource,
        })
    }
}

pub fn output_bundle_macro(input: PseudoStruct::<Entry>) -> TokenStream {
    let render_graph = crate::get_crate_path();

    let PseudoStruct::<Entry> {
        visibility,
        name: struct_name,
        fields,
    } = input;

    let struct_fields = match &fields {
        PseudoStructFields::Named(fields) => {
            let handle_fields = fields.iter().filter(|field| field.val.resource.needs_dynamic_handle()).map(|field| {
                let name = &field.name;
                let ty = field.val.resource.handle_type();
                quote!{ pub #name: #ty }
            });
            quote!{ { #(#handle_fields,)* } }
        },
        PseudoStructFields::Unnamed(fields) => {
            let handle_fields = fields.iter().filter(|entry| entry.resource.needs_dynamic_handle()).map(|entry| {
                let ty = entry.resource.handle_type();
                quote!{ pub #ty }
            });
            quote!{ ( #(#handle_fields,)* ); }
        },
        PseudoStructFields::Unit => quote!{ ; },
    };

    let value_struct_name = format_ident!("{struct_name}Value");
    let value_struct_fields = match &fields {
        PseudoStructFields::Named(fields) => {
            let handle_fields = fields.iter().filter(|f| !f.val.is_default).map(|field| {
                let name = &field.name;
                let ty = field.val.resource.value_type(None);
                quote!{ #name: #ty }
            });
            quote!{ { #(#handle_fields,)* } }
        },
        PseudoStructFields::Unnamed(fields) => {
            let handle_fields = fields.iter().filter(|entry| !entry.is_default).map(|entry| entry.resource.value_type(None));
            quote!{ ( #(#handle_fields,)* ); }
        },
        PseudoStructFields::Unit => quote!{ ; },
    };

    let struct_fields_names = match &fields {
        PseudoStructFields::Named(fields) => fields.iter().map(|field| {
            let name = &field.name;
            quote!{ #name }
        }).collect::<Box<[_]>>(),
        PseudoStructFields::Unnamed(entries) => entries.iter().scan(0usize, |idx, entry| {
            if entry.resource.needs_dynamic_handle() {
                let i = syn::Index::from(std::mem::replace(idx, idx.checked_add(1).unwrap()));
                Some(quote!{ #i })
            }
            else {
                Some(quote!{})
            }
        }).collect::<Box<[_]>>(),
        PseudoStructFields::Unit => Box::<[_]>::default(),
    };
    let value_struct_fields_names = match &fields {
        PseudoStructFields::Named(fields) => fields.iter().map(|field| {
            let name = &field.name;
            quote!{ #name }
        }).collect::<Box<[_]>>(),
        PseudoStructFields::Unnamed(entries) => entries.iter().scan(0usize, |idx, entry| {
            if entry.is_default {
                Some(quote!{})
            } else {
                let i = syn::Index::from(std::mem::replace(idx, idx.checked_add(1).unwrap()));
                Some(quote!{ #i })
            }
        }).collect::<Box<[_]>>(),
        PseudoStructFields::Unit => Box::<[_]>::default(),
    };

    let handles = zip(&struct_fields_names, fields.iter()).map(|(field_name, entry)| {
        match &entry.resource {
            EntryResource::FromType(t) => quote!{ storer.resource_from_type::<#t>() },
            EntryResource::Dynamic(_) | EntryResource::Untyped => quote!{ self.#field_name },
        }
    });
    let resource_count = fields.iter().count();

    let gathered_resources_vars = handles.clone().enumerate().map(|(i, resource)| {
        let var_name = format_ident!("resource{i}");
        quote!{ let #var_name = #resource; }
    });

    let values_store = fields.iter().enumerate().zip(&value_struct_fields_names).map(|((i, entry), value_field_name)| {
        let var_name = format_ident!("resource{i}");
        let val = if entry.is_default {
            let ty = entry.resource.value_type(None);
            quote!{ <#ty as ::std::default::Default>::default() }
        } else {
            quote!{ values.#value_field_name }
        };
        match entry.resource {
            EntryResource::FromType(_) | EntryResource::Dynamic(_) => {
                quote!{ storer.store(#var_name, #val); }
            },
            EntryResource::Untyped => {
                quote!{ storer.store_untyped(#var_name, #val); }
            },
        }
    });

    let struct_derives = if fields.iter().all(|entry| matches!(&entry.resource, EntryResource::FromType(_))) {
        quote!{ #[derive(::std::default::Default)] }
    } else {
        quote! {}
    };

    let value_struct_derives = if fields.iter().all(|entry| entry.is_default) {
        quote!{ #[derive(::std::default::Default)] }
    } else {
        quote! {}
    };

    quote! {
        #struct_derives
        #visibility struct #struct_name #struct_fields
        #value_struct_derives
        #visibility struct #value_struct_name #value_struct_fields

        #[automatically_derived]
        impl #render_graph::ResourceOutputBundle for #struct_name {
            type Values = #value_struct_name;

            #[inline]
            fn list_resources(&self, storer: &mut impl #render_graph::ResourceInfoProvider) -> [#render_graph::UntypedResourceHandle; #resource_count] {
                [#(#handles.into()),*]
            }

            #[inline]
            fn store(&self, values: Self::Values, storer: &mut impl #render_graph::ResourceStorer) {
                #(#gathered_resources_vars)*
                #(#values_store)*
            }
        }
    }
}
