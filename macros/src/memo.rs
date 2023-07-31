use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse2, parse_macro_input,
    punctuated::Punctuated,
    Expr, Ident, Result, Token,
};

struct Memo {
    args: Vec<Ident>,
    body: Expr,
}

impl Parse for Memo {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<Token! {|}>()?;
        let args = Punctuated::<Ident, Token![,]>::parse_separated_nonempty(input)?;
        input.parse::<Token! {|}>()?;
        let body = input.parse()?;
        Ok(Memo {
            args: args.into_iter().collect(),
            body,
        })
    }
}

pub fn memo(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input);
    let output: proc_macro2::TokenStream = { map_int(input) };
    proc_macro::TokenStream::from(output)
}

pub fn map_int(input: TokenStream) -> TokenStream {
    let map: Memo = parse2(input).unwrap();
    let identifiers: Vec<_> = map.args.iter().collect();
    let first = &map.args.first();
    let getters: Vec<_> = map
        .args
        .iter()
        .map(|ident| {
            quote! { #ident.get() }
        })
        .collect();
    let body = &map.body;

    quote! {
        {
            #(let #identifiers = #identifiers.clone();)*
            #first.runtime().memo(
                move || (#(#getters,)*)
                ,
                move |(#(#identifiers,)*)| {
                #body
            })
        }
    }
}
