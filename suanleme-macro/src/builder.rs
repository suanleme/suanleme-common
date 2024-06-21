use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Data, DeriveInput};

pub fn builder(item: TokenStream) -> TokenStream {
    let org_item = parse_macro_input!(item as DeriveInput);
    let ident = &org_item.ident;
    let builder = syn::Ident::new(&format!("{}Builder", ident), ident.span());
    let Data::Struct(data_struct) = &org_item.data else {
        return syn::Error::new_spanned(org_item.to_token_stream(), "builder must label to struct")
            .into_compile_error()
            .into();
    };
    let fields_builder = data_struct.fields.iter().fold(vec![], |mut vec, e| {
        let ident = e.ident.as_ref().unwrap();
        let _type = e.ty.to_token_stream();
        vec.push(quote!(
            pub fn #ident(mut self,#ident : #_type) -> Self {
                self.cache.#ident = #ident;
                self
            }
        ));
        vec
    });
    let token = quote! {

        #[derive(Default)]
        #org_item

        pub struct #builder {
            cache : #ident
        }

        impl #ident {
            pub fn builder() -> #builder {
                return #builder {cache :
                   #ident::default()
                };
            }
        }

        impl #builder {
            #(
               #fields_builder
            )*
            pub fn build(self) -> #ident {
                self.cache
            }
        }
    };
    token.into()
}
