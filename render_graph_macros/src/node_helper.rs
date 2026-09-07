use convert_case::ccase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, Token, parenthesized, parse::Parse, parse_quote};

use crate::{PseudoStruct, PseudoStructFields, input_bundle::{self, input_bundle_macro}, output_bundle::{self, output_bundle_macro}};

mod kw {
    syn::custom_keyword!(into);
    syn::custom_keyword!(using);
    syn::custom_keyword!(permanent);
}

enum InputEntryOrAliased {
    Entry(input_bundle::Entry),
    Aliased { is_ref: bool, alias_name: syn::Ident },
}

impl InputEntryOrAliased {
    fn alias_name(&self) -> Option<&syn::Ident> {
        if let Self::Aliased { alias_name, .. } = self {
            Some(alias_name)
        }
        else{
            None
        }
    }
}

impl Parse for InputEntryOrAliased {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.peek(Token![@]) || (input.peek(Token![ref]) && input.peek2(Token![@])) {
            let is_ref = if input.peek(Token![ref]) {
                input.parse::<Token![ref]>()?;
                true
            }
            else { false };
            input.parse::<Token![@]>()?;
            let alias_name = input.parse::<syn::Ident>()?;
            Ok(Self::Aliased {
                is_ref,
                alias_name,
            })
        }
        else {
            Ok(Self::Entry(input_bundle::Entry::parse_no_ignore(input)?))
        }
    }
}

enum OutputEntryOrAliased {
    Entry(output_bundle::Entry),
    Aliased { is_default: bool, alias_name: syn::Ident },
}

impl OutputEntryOrAliased {
    fn is_default(&self) -> bool {
        match self {
            Self::Entry(entry) => entry.is_default,
            &Self::Aliased { is_default, .. } => is_default,
        }
    }

    fn alias_name(&self) -> Option<&syn::Ident> {
        if let Self::Aliased { alias_name, .. } = self {
            Some(alias_name)
        }
        else{
            None
        }
    }
}

impl Parse for OutputEntryOrAliased {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.peek(Token![@]) || (input.peek(super::kw::default) && input.peek2(Token![@])) {
            let is_default = if input.peek(super::kw::default) {
                input.parse::<super::kw::default>()?;
                true
            }
            else { false };
            input.parse::<Token![@]>()?;
            let alias_name = input.parse::<syn::Ident>()?;
            Ok(Self::Aliased {
                is_default,
                alias_name,
            })
        }
        else {
            Ok(Self::Entry(input.parse()?))
        }
    }
}

struct InputField {
    pat: syn::Pat,
    input_entry: InputEntryOrAliased,
}

impl Parse for InputField {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let pat = syn::Pat::parse_single(input)?;
        input.parse::<Token![:]>()?;
        let input_entry = input.parse::<InputEntryOrAliased>()?;
        Ok(Self {
            pat,
            input_entry,
        })
    }
}

struct OutputField {
    output_entry: OutputEntryOrAliased,
}

impl Parse for OutputField {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            output_entry: input.parse::<OutputEntryOrAliased>()?,
        })
    }
}

struct ResourceAliaseEntry {
    name: Ident,
    ty: syn::Type,
    expr: syn::Expr,
}

impl Parse for ResourceAliaseEntry {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        input.parse::<kw::using>()?;
        input.parse::<Token![@]>()?;
        let name = input.parse::<Ident>()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse::<syn::Type>()?;
        input.parse::<Token![=]>()?;
        let expr = input.parse::<syn::Expr>()?;
        input.parse::<Token![;]>()?;

        Ok(Self {
            name,
            ty,
            expr,
        })
    }
}

struct Entry {
    name: Ident,
    input_fields: Box<[InputField]>,
    output_fields: Box<[OutputField]>,
    code: Option<syn::Block>,
}

impl Parse for Entry {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;

        let input_content;
        parenthesized!(input_content in input);
        let input_fields = input_content.parse_terminated(InputField::parse, Token![,])?.into_iter().collect();

        input.parse::<Token![->]>()?;

        let output_content;
        parenthesized!(output_content in input);
        let output_fields = output_content.parse_terminated(OutputField::parse, Token![,])?.into_iter().collect::<Box<[_]>>();

        let code = if output_fields.iter().all(|e| e.output_entry.is_default()) && !input.peek(syn::token::Brace) {
            None
        }
        else {
            Some(input.parse()?)
        };

        Ok(Self {
            name,
            input_fields,
            output_fields,
            code,
        })
    }
}

pub struct Input {
    graph_ident: Ident,
    resource_aliases: Box<[ResourceAliaseEntry]>,
    entries: Box<[Entry]>,
}

impl Input {
    fn alias_to_resource(&self, alias: &Ident) -> super::EntryResource {
        let ty = self.resource_aliases.iter().find(|a| &a.name == alias).expect("could not find an alias").ty.clone();
        crate::EntryResource::Dynamic(ty)
    }
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        input.parse::<kw::into>()?;
        let graph_ident = input.parse::<Ident>()?;
        input.parse::<Token![;]>()?;

        let mut resource_aliases = Vec::<ResourceAliaseEntry>::new();
        while input.peek(kw::using) {
            resource_aliases.push(input.parse()?);
        }

        let entries = input.parse_terminated(Entry::parse, Token![;])?.into_iter().collect();
        Ok(Self {
            graph_ident,
            resource_aliases: resource_aliases.into_boxed_slice(),
            entries,
        })
    }
}

#[expect(clippy::single_call_fn, reason = "Big code block better exctacted into function")]
fn process_entry(input: &Input, entry: &Entry) -> TokenStream {
    let graph_ident = &input.graph_ident;
    let render_graph = super::get_crate_path();

    let input_entries = entry.input_fields.iter().map(|e| {
        let mut input_entry = match &e.input_entry {
            InputEntryOrAliased::Entry(input_entry) => input_entry.clone(),
            InputEntryOrAliased::Aliased { is_ref, alias_name } => input_bundle::Entry {
                is_ignored: false,
                kind: if *is_ref { input_bundle::EntryKind::Borrow } else { input_bundle::EntryKind::Consume },
                resource: input.alias_to_resource(alias_name),
            },
        };
        input_entry.is_ignored = matches!(e.pat, syn::Pat::Wild(_));
        input_entry
    }).collect();
    let output_entries = entry.output_fields.iter().map(|e| {
        match &e.output_entry {
            OutputEntryOrAliased::Entry(output_entry) => output_entry.clone(),
            &OutputEntryOrAliased::Aliased { is_default, ref alias_name } => output_bundle::Entry {
                is_default,
                resource: input.alias_to_resource(alias_name),
            },
        }
    }).collect::<Vec<_>>();

    let mut body = entry.code.clone().unwrap_or_else(|| parse_quote!({  }));
    if output_entries.iter().all(|e| e.is_default) {
        body = parse_quote!({
            #body
            ::std::default::Default::default()
        });
    }

    let input_bundle = input_bundle_macro(PseudoStruct::<input_bundle::Entry> {
        visibility: syn::Visibility::Public(syn::token::Pub { span: Span::call_site() }),
        name: format_ident!("Input"),
        fields: PseudoStructFields::Unnamed(input_entries),
    });
    let output_bundle = output_bundle_macro(PseudoStruct::<output_bundle::Entry> {
        visibility: syn::Visibility::Public(syn::token::Pub { span: Span::call_site() }),
        name: format_ident!("Output"),
        fields: PseudoStructFields::Unnamed(output_entries),
    });

    let node_name = &entry.name;
    let snake_node_name = &ccase!(pascal -> snake, node_name.to_string());
    let module_name = syn::Ident::new(&format!("{snake_node_name}_mod"), node_name.span());
    let var_name = syn::Ident::new(snake_node_name, node_name.span());

    let arg_pats = entry.input_fields.iter().map(|e| &e.pat).filter(|pat| !matches!(pat, syn::Pat::Wild(_)));
    
    let input_dyns = entry.input_fields.iter().filter_map(|p| p.input_entry.alias_name()).map(|name| format_ident!("{name}_resource"));
    let output_dyns = entry.output_fields.iter().filter_map(|p| p.output_entry.alias_name()).map(|name| format_ident!("{name}_resource"));

    quote! {
        mod #module_name {
            use super::*;

            #input_bundle
            #output_bundle

            pub struct #node_name;
            impl #render_graph::GraphNode for #node_name {
                type InputBundle = Input;
                type OutputBundle = Output;
                fn run(&mut self, InputValue(#(#arg_pats),*): InputValue) -> OutputValue #body
            }
        }

        #[allow(unused_variables)]
        let #var_name = #graph_ident.push_node_complete(#module_name::#node_name, #module_name::Input(#(#input_dyns),*), #module_name::Output(#(#output_dyns),*));
    }
}

#[expect(clippy::single_call_fn, reason = "Macro implementation")]
pub fn node_helper_macro(input: &Input) -> TokenStream {
    let render_graph = super::get_crate_path();

    let res_names = input.resource_aliases.iter().map(|p| &p.name).map(|name| format_ident!("{name}_resource"));
    let res_exprs = input.resource_aliases.iter().map(|p| &p.expr);
    let res_tys = input.resource_aliases.iter().map(|p| &p.ty);

    let entries = input.entries.iter().map(|entry| process_entry(input, entry));

    quote!{
        #(let #res_names: #render_graph::ResourceHandle<#res_tys> = #res_exprs;)*

        #(#entries)*
    }
}
