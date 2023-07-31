use proc_macro::TokenStream;

mod map;
mod memo;

#[proc_macro]
pub fn map(input: TokenStream) -> TokenStream {
    map::map(input)
}

#[proc_macro]
pub fn memo(input: TokenStream) -> TokenStream {
    memo::memo(input)
}
