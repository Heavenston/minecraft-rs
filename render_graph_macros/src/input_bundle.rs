use std::iter::zip;

use itertools::{ Either, Itertools as _ };
use proc_macro2::{ TokenStream };
use quote::{ format_ident, quote };
use syn::{ Token, parse::Parse, parse_quote };

use crate::{EntryResource, PseudoStruct, PseudoStructFields, kw};

enum EntryKind {
    Borrow,
    Consume,
}

pub struct Entry {
    is_ignored: bool,
    kind: EntryKind,
    resource: EntryResource,
}

impl Entry {
    #[expect(clippy::single_call_fn, reason="?")]
    fn value_type(&self) -> syn::Type {
        match self.kind {
            EntryKind::Borrow => self.resource.value_type(Some(&parse_quote!{ 'a })),
            EntryKind::Consume => self.resource.value_type(None),
        }
    }
}

impl Parse for Entry {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let is_ignored = if input.peek(kw::ignore) { input.parse::<kw::ignore>()?; true } else { false };
        let kind = if input.peek(Token![ref]) {
            input.parse::<Token![ref]>()?;
            EntryKind::Borrow
        } else {
            EntryKind::Consume
        };
        let resource = input.parse()?;

        Ok(Self {
            is_ignored,
            kind,
            resource,
        })
    }
}

#[expect(clippy::single_call_fn, reason = "only used inside lib.rs macro function")]
pub fn input_bundle_macro(input: PseudoStruct<Entry>) -> TokenStream {
    let render_graph = super::get_crate_path();

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
                quote!{ #name: #ty }
            });
            quote!{ { #(#handle_fields,)* } }
        },
        PseudoStructFields::Unnamed(fields) => {
            let handle_fields = fields.iter().filter(|entry| entry.resource.needs_dynamic_handle()).map(|entry| {
                let ty = entry.resource.handle_type();
                quote!{ #ty }
            });
            quote!{ ( #(#handle_fields,)* ); }
        },
        PseudoStructFields::Unit => quote!{ ; },
    };

    let value_struct_name = format_ident!("{struct_name}Value");
    let value_struct_fields = match &fields {
        PseudoStructFields::Named(fields) => {
            let handle_fields = fields.iter().filter(|entry| !entry.val.is_ignored).map(|field| {
                let name = &field.name;
                let ty = field.val.value_type();
                quote!{ #name: #ty }
            });
            quote!{ { #(#handle_fields,)* } }
        },
        PseudoStructFields::Unnamed(fields) => {
            let handle_fields = fields.iter().filter(|entry| !entry.is_ignored).map(Entry::value_type);
            quote!{ ( #(#handle_fields,)* ); }
        },
        PseudoStructFields::Unit => quote!{ ; },
    };
    let value_struct_gen_params = if fields.iter().any(|entry| !entry.is_ignored && match entry.kind {
        EntryKind::Borrow => true,
        EntryKind::Consume => false,
    }) {
        quote!{ <'a> }
    } else {
        quote!{}
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

    let resource_handles = fields.iter().zip(&struct_fields_names).map(|(entry, field_name)| {
        match &entry.resource {
            EntryResource::FromType(t) => quote!{ gatherer.resource_from_type::<#t>() },
            EntryResource::Dynamic(_) | EntryResource::Untyped => quote!{ self.#field_name },
        }
    });

    let borrowed_resource_handles = zip(resource_handles.clone(), fields.iter()).filter_map(|(res, entry)| matches!(entry.kind, EntryKind::Borrow).then_some(res));
    let borrowed_count = borrowed_resource_handles.clone().count();
    let consumed_resource_handles = zip(resource_handles.clone(), fields.iter()).filter_map(|(res, entry)| matches!(entry.kind, EntryKind::Consume).then_some(res));
    let consumed_count = consumed_resource_handles.clone().count();

    let gathered_handles_vars = resource_handles.clone().enumerate().map(|(i, resource)| {
        let var_name = format_ident!("handle{i}");
        quote!{ let #var_name = #resource; }
    });

    // Consuming must happen before any borrowing, as consuming requires mutable
    // access to the gatherer, whose lifetime would be in use by the borrows
    let (
        borrow_resources_values_gather,
        consume_resources_values_gather,
    ): (TokenStream, TokenStream) = fields.iter().enumerate().partition_map(|(i, entry)| {
        let handle_var_name = format_ident!("handle{i}");
        let value_var_name = format_ident!("resource{i}");
        match (&entry.kind, &entry.resource) {
            (EntryKind::Borrow, EntryResource::FromType(_) | EntryResource::Dynamic(_))
                => Either::Left(quote!{ let #value_var_name = gatherer.borrow(#handle_var_name); }),
            (EntryKind::Borrow, EntryResource::Untyped)
                => Either::Left(quote!{ let #value_var_name = gatherer.borrow_untyped(#handle_var_name); }),
            (EntryKind::Consume, EntryResource::FromType(_) | EntryResource::Dynamic(_))
                => Either::Right(quote!{ let #value_var_name = gatherer.consume(#handle_var_name); }),
            (EntryKind::Consume, EntryResource::Untyped)
                => Either::Right(quote!{ let #value_var_name = gatherer.consume_untyped(#handle_var_name); }),
        }
    });

    let value_struct_construction = match &fields {
        PseudoStructFields::Named(fields) => {
            let field_names = fields.iter().filter(|field| !field.val.is_ignored).map(|field| &field.name);
            let value_names = fields.iter().enumerate().filter(|(_, field)| !field.val.is_ignored).map(|(i,_)| format_ident!("resource{i}"));
            quote!{ #value_struct_name {
                #(#field_names: #value_names,)*
            } }
        },
        PseudoStructFields::Unnamed(fields) => {
            let value_names = fields.iter().enumerate().filter(|(_, field)| !field.is_ignored).map(|(i,_)| format_ident!("resource{i}"));
            quote!{ #value_struct_name(#(#value_names,)*) }
        },
        PseudoStructFields::Unit => quote!{ #value_struct_name },
    };

    let struct_derives = if fields.iter().all(|p| matches!(&p.resource, EntryResource::FromType(_))) {
        quote!{ #[derive(::std::default::Default)] }
    } else {
        quote! {}
    };

    quote! {
        #struct_derives
        #visibility struct #struct_name #struct_fields
        struct #value_struct_name #value_struct_gen_params #value_struct_fields

        #[automatically_derived]
        impl #render_graph::ResourceInputBundle for #struct_name {
            type Values<'a> = #value_struct_name #value_struct_gen_params;

            fn list_consumes(&self, gatherer: &mut impl #render_graph::ResourceInfoProvider) -> [#render_graph::UntypedResourceHandle; #consumed_count] {
                [#(#consumed_resource_handles.into()),*]
            }

            fn list_borrows(&self, gatherer: &mut impl #render_graph::ResourceInfoProvider) -> [#render_graph::UntypedResourceHandle; #borrowed_count] {
                [#(#borrowed_resource_handles.into()),*]
            }

            fn gather<'a>(&self, gatherer: &'a mut impl #render_graph::ResourceGatherer) -> Self::Values<'a> {
                #(#gathered_handles_vars)*
                #consume_resources_values_gather
                #borrow_resources_values_gather
                #value_struct_construction
            }
        }
    }
}
