use proc_macro::TokenStream;

mod builder;
mod config;
mod data;

#[proc_macro_attribute]
pub fn builder(_attr: TokenStream, item: TokenStream) -> TokenStream {
    builder::builder(item)
}

#[proc_macro_attribute]
pub fn hot_config(_attr: TokenStream, item: TokenStream) -> TokenStream {
    config::hot_config(item)
}

#[proc_macro_derive(Data)]
pub fn data(item: TokenStream) -> TokenStream {
    data::data(item)
}
