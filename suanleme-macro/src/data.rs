use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{parse_macro_input, Data, DeriveInput};

pub fn data(item: TokenStream) -> TokenStream {
    let org_item = parse_macro_input!(item as DeriveInput);
    let ident = &org_item.ident;
    let (generics_0, generics_1, generics_2) = org_item.generics.split_for_impl();
    let Data::Struct(data_struct) = &org_item.data else {
        return syn::Error::new_spanned(org_item.to_token_stream(), "builder must label to struct")
            .into_compile_error()
            .into();
    };
    let fields_builder = data_struct.fields.iter().fold(vec![], |mut vec, e| {
        let ident = e.ident.as_ref().unwrap();
        let _type = e.ty.to_token_stream();
        let get_name = format_ident!("get_{}", ident.to_string());
        vec.push(quote!(
            pub fn #ident(mut self,#ident : #_type) -> Self {
                self.#ident = #ident;
                self
            }
            pub fn #get_name(&self) -> &#_type {
                &self.#ident
            }
        ));
        vec
    });
    let token = quote! {
        impl #generics_0 #ident #generics_1
        #generics_2
        {

            #(#fields_builder)*

            pub fn builder() -> Self {
                return Default::default();
            }
        }
    };
    token.into()
}
