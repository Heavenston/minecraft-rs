use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::{Ident, Token, Visibility, braced, parenthesized, parse::Parse, parse_macro_input, parse_quote};
use quote::quote;

mod input_bundle;
mod output_bundle;
mod node_helper;

fn get_crate_path() -> proc_macro2::TokenStream {
    if cfg!(test) {
        return quote!{ render_graph };
    }
    match proc_macro_crate::crate_name("render_graph").expect("render_graph is present in Cargo.toml") {
        proc_macro_crate::FoundCrate::Itself => quote! { crate },
        proc_macro_crate::FoundCrate::Name(name) => {
            let ident = Ident::new(&name, Span::call_site());
            quote! { #ident }
        }
    }
}

mod kw {
    syn::custom_keyword!(untyped);
    syn::custom_keyword!(ignore);
    syn::custom_keyword!(default);
}

#[derive(Clone)]
enum EntryResource {
    FromType(syn::Type),
    Dynamic(syn::Type),
    Untyped,
}

impl EntryResource {
    fn needs_dynamic_handle(&self) -> bool {
        match self {
            Self::FromType(_) => false,
            Self::Dynamic(_) | Self::Untyped => true,
            }
    }

    fn handle_type(&self) -> syn::Type {
        let render_graph = get_crate_path();
        match self {
            Self::FromType(_) | Self::Dynamic(_) => {
                let ty = self.value_type(None);
                parse_quote! { #render_graph::ResourceHandle<#ty> }
            },
            Self::Untyped => {
                parse_quote! { #render_graph::UntypedResourceHandle }
            },
        }
    }

    fn value_type(&self, ref_lifetime: Option<&syn::Lifetime>) -> syn::Type {
        let render_graph = get_crate_path();

        match (ref_lifetime, self) {
            (None, Self::FromType(t)) => parse_quote!{ <#t as #render_graph::GraphResourceId>::Resource },
            (Some(lt), Self::FromType(t)) => parse_quote!{ &#lt <#t as #render_graph::GraphResourceId>::Resource },
            (None, Self::Dynamic(t)) => parse_quote!{ #t },
            (Some(lt), Self::Dynamic(t)) => parse_quote!{ &#lt #t },
            (None, Self::Untyped) => parse_quote!{ Box<dyn ::std::any::Any> },
            (Some(lt), Self::Untyped) => parse_quote!{ &#lt dyn ::std::any::Any },
        }
    }
}

impl Parse for EntryResource {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(if input.peek(Token![dyn]) {
            input.parse::<Token![dyn]>()?;
            Self::Dynamic(input.parse()?)
        }
        else if input.peek(kw::untyped) {
            input.parse::<kw::untyped>()?;
            Self::Untyped
        }
        else {
            Self::FromType(input.parse()?)
        })
    }
}

#[derive(Clone)]
struct PseudoStructNamedField<E> {
    name: Ident,
    val: E,
}

impl<E: Parse> Parse for PseudoStructNamedField<E> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let val = input.parse()?;
        Ok(Self {
            name,
            val,
        })
    }
}

#[derive(Clone)]
enum PseudoStructFields<E> {
    Named(Vec<PseudoStructNamedField<E>>),
    Unnamed(Vec<E>),
    Unit,
}

impl<E> PseudoStructFields<E> {
    fn iter<'a>(&'a self) -> impl Iterator<Item = &'a E> + Clone {
        let mapper_a = |f: &'a PseudoStructNamedField<E>| -> &'a E { &f.val };
        let (a, b) = match self {
            Self::Named(i) => (i.iter().map(mapper_a),[].iter()),
            Self::Unnamed(i) => ([].iter().map(mapper_a),i.iter()),
            Self::Unit => ([].iter().map(mapper_a),[].iter()),
        };
        std::iter::chain(a, b)
    }
}

impl<E: Parse> Parse for PseudoStructFields<E> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            Self::Named(content.parse_terminated(PseudoStructNamedField::parse, Token![,])?.into_iter().collect())
        } else if input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in input);
            Self::Unnamed(content.parse_terminated(E::parse, Token![,])?.into_iter().collect())
        } else {
            Self::Unit
        })
    }
}

struct PseudoStruct<E> {
    visibility: Visibility,
    name: Ident,
    fields: PseudoStructFields<E>,
}

impl<E: Parse> Parse for PseudoStruct<E> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let visibility = input.parse()?;
        input.parse::<Token![struct]>()?;
        let name = input.parse()?;
        let fields = input.parse()?;
        Ok(Self {
            visibility,
            name,
            fields,
        })
    }
}

#[proc_macro]
pub fn input_bundle(input: TokenStream) -> TokenStream {
    input_bundle::input_bundle_macro(parse_macro_input!(input)).into()
}

#[proc_macro]
pub fn output_bundle(input: TokenStream) -> TokenStream {
    output_bundle::output_bundle_macro(parse_macro_input!(input)).into()
}

#[proc_macro]
pub fn node_helper(input: TokenStream) -> TokenStream {
    node_helper::node_helper_macro(&parse_macro_input!(input)).into()
}
