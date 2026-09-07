use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, Token, parenthesized, parse::Parse, parse_quote};

use crate::{PseudoStruct, PseudoStructFields, input_bundle::{self, input_bundle_macro}, output_bundle::{self, output_bundle_macro}};

mod kw {
    syn::custom_keyword!(into);
}

struct InputField {
    pat: syn::Pat,
    input_entry: input_bundle::Entry,
}

impl Parse for InputField {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let pat = syn::Pat::parse_single(input)?;
        input.parse::<Token![:]>()?;
        let input_entry = input_bundle::Entry::parse_no_ignore(input)?;
        Ok(Self {
            pat,
            input_entry,
        })
    }
}

struct Entry {
    name: Ident,
    input: Box<[InputField]>,
    output: PseudoStructFields<output_bundle::Entry>,
    code: Option<syn::Block>,
}

impl Parse for Entry {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        let input_content;
        parenthesized!(input_content in input);
        let input_fields = input_content.parse_terminated(InputField::parse, Token![,])?.into_iter().collect();
        input.parse::<Token![->]>()?;
        let output: PseudoStructFields<output_bundle::Entry> = input.parse()?;
        let code = if output.value_impl_default() && !input.peek(syn::token::Brace) {
            None
        }
        else {
            Some(input.parse()?)
        };
        Ok(Self {
            name,
            input: input_fields,
            output,
            code,
        })
    }
}

pub struct Input {
    graph_ident: Ident,
    entries: Box<[Entry]>,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        input.parse::<kw::into>()?;
        let graph_ident = input.parse::<Ident>()?;
        input.parse::<Token![;]>()?;
        let entries = input.parse_terminated(Entry::parse, Token![;])?.into_iter().collect();
        Ok(Self {
            graph_ident,
            entries,
        })
    }
}

#[expect(clippy::single_call_fn, reason = "Big code block better exctacted into function")]
fn process_entry(graph_ident: &Ident, entry: &Entry) -> TokenStream {
    let render_graph = super::get_crate_path();

    let input_fields = entry.input.iter().map(|e| {
        let is_wild = matches!(e.pat, syn::Pat::Wild(_));
        let mut input_entry = e.input_entry.clone();
        input_entry.is_ignored = is_wild;
        input_entry
    }).collect();
    let input_bundle = input_bundle_macro(PseudoStruct::<input_bundle::Entry> {
        visibility: syn::Visibility::Public(syn::token::Pub { span: Span::call_site() }),
        name: format_ident!("Input"),
        fields: PseudoStructFields::Unnamed(input_fields),
    });
    let output_bundle = output_bundle_macro(PseudoStruct::<output_bundle::Entry> {
        visibility: syn::Visibility::Public(syn::token::Pub { span: Span::call_site() }),
        name: format_ident!("Output"),
        fields: entry.output.clone(),
    });

    let node_name = &entry.name;
    let module_name = format_ident!("{}_mod", entry.name);
    let mut body = entry.code.clone().unwrap_or_else(|| parse_quote!({  }));
    if entry.output.value_impl_default() {
        body = parse_quote!({
            #body
            ::std::default::Default::default()
        });
    }

    let arg_pats = entry.input.iter().map(|e| &e.pat).filter(|pat| !matches!(pat, syn::Pat::Wild(_)));

    quote! {
        #[allow(non_snake_case)]
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
        #graph_ident.push_node(#module_name::#node_name);
    }
}

#[expect(clippy::single_call_fn, reason = "Macro implementation")]
pub fn node_helper_macro(input: Input) -> TokenStream {
    input.entries.into_iter().map(|entry| process_entry(&input.graph_ident, &entry)).collect()
}
