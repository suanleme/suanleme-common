use proc_macro::TokenStream;
use quote::ToTokens;
use suanleme_derive_macro::fusen_attr;
use syn::parse::Parser;

mod builder;
mod config;
mod data;
mod debug;

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

#[proc_macro_derive(StrategyDebug, attributes(strategy))]
pub fn strategy_debug(item: TokenStream) -> TokenStream {
    debug::debug(item)
}

fusen_attr! {
    StrategyAttr,
    ignore,
    limit,
    mask
}
