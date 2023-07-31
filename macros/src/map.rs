use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse2, parse_macro_input,
    punctuated::Punctuated,
    Expr, Ident, Result, Token,
};

enum ArgType {
    Reference,
    Value,
}

struct Arg {
    ty: ArgType,
    ident: Ident,
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        let ty = if input.peek(Token![&]) {
            input.parse::<Token![&]>()?;
            ArgType::Reference
        } else {
            ArgType::Value
        };
        let ident = input.parse()?;
        Ok(Arg { ty, ident })
    }
}

struct Map {
    args: Vec<Arg>,
    body: Expr,
}

impl Parse for Map {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<Token! {|}>()?;
        let args = Punctuated::<Arg, Token![,]>::parse_separated_nonempty(input)?;
        input.parse::<Token! {|}>()?;
        let body = input.parse()?;
        Ok(Map {
            args: args.into_iter().collect(),
            body,
        })
    }
}

pub fn map(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input);
    let output: proc_macro2::TokenStream = { map_int(input) };
    proc_macro::TokenStream::from(output)
}

pub fn map_int(input: TokenStream) -> TokenStream {
    let map: Map = parse2(input).unwrap();
    let identifiers: Vec<_> = map.args.iter().map(|a| &a.ident).collect();
    let first = &map.args.first().unwrap().ident;
    let getters = map.args.iter().map(|a| {
        let ident = &a.ident;
        match a.ty {
            ArgType::Reference => quote! { &*#ident.get_ref() },
            ArgType::Value => quote! { #ident.get() },
        }
    });
    let body = &map.body;

    quote! {
        {
            #(let #identifiers = #identifiers.clone();)*
            #first.runtime().computed(move || {
                #(let #identifiers = #getters;)*
                #body
            })
        }
    }
}
