use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

pub fn enum_for_str(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let enum_name = input.ident;

    let ret = match input.data {
        syn::Data::Enum(enum_data) => {
            let ed = enum_data.variants.iter().map(|e| {
                let var_name = &e.ident;
                let var_name_str = e.ident.to_string();
                quote! {
                  #enum_name::#var_name=>#var_name_str,
                }
            });

            quote! {
                impl #enum_name {
                  pub fn to_str(&self) -> &'static str {
                        match self {
                          #(#ed)*
                        }
                  }
                }

                impl From<#enum_name> for &'static str {
                  fn from(value: #enum_name) -> Self {
                      value.to_str()
                  }
                }

                impl From<#enum_name> for String {
                    fn from(value: #enum_name) -> Self {
                        value.to_str().to_string()
                    }
                }

                impl PartialEq<&str> for #enum_name {
                    fn eq(&self, other: &&str) -> bool {
                        *other == self.to_str()
                    }
                }

                impl PartialEq<String> for #enum_name {
                    fn eq(&self, other: &String) -> bool {
                        *other == self.to_str()
                    }
                }

                impl PartialEq<#enum_name> for String {
                    fn eq(&self, other: &#enum_name) -> bool {
                        self == other.to_str()
                    }
                }
            }
        }
        _ => syn::Error::new(enum_name.span(), "only be used enum")
            .into_compile_error()
            .into(),
    };
    ret.into()
}
