use proc_macro::TokenStream;

mod builder;

#[proc_macro_attribute]
pub fn builder(_attr: TokenStream, item: TokenStream) -> TokenStream {
    builder::builder(item)
}
