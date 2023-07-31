use proc_macro::TokenStream;

mod map;


#[proc_macro]
pub fn map(input: TokenStream) -> TokenStream {
    map::map(input)
}
