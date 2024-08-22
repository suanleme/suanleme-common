use crate::StrategyAttr;
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Attribute, Data, DeriveInput, Meta};

pub fn debug(item: TokenStream) -> TokenStream {
    let org_item = parse_macro_input!(item as DeriveInput);
    let ident = &org_item.ident;
    let (generics_0, generics_1, generics_2) = org_item.generics.split_for_impl();
    let generics = org_item.generics.params.iter().fold(vec![], |mut vec, e| {
        vec.push(e.to_token_stream());
        vec
    });
    let generics_2 = match generics_2 {
        Some(whrere_generics) => {
            let point = if whrere_generics.predicates.empty_or_trailing() {
                quote! {}
            } else {
                quote! {,}
            };
            quote! {
                #whrere_generics
                #point
                #(
                    #generics : std::fmt::Debug,
                )*
            }
        }
        None => quote! {
            where
            #(
                #generics : std::fmt::Debug,
            )*
        },
    };
    let Data::Struct(data_struct) = &org_item.data else {
        return syn::Error::new_spanned(org_item.to_token_stream(), "debug must label to struct")
            .into_compile_error()
            .into();
    };
    let mut fields = vec![];
    for field in &data_struct.fields {
        let ident = field.ident.as_ref().unwrap();
        let _type = field.ty.to_token_stream();
        let mut ident_name = ident.to_string();
        if ident_name.starts_with("r#") {
            ident_name = ident_name[2..ident_name.len()].to_string();
        }
        let strategy = match get_strategy_by_attrs(&field.attrs) {
            Ok(strategy) => strategy,
            Err(error) => return error.into_compile_error().into(),
        };
        if strategy.ignore.is_some() {
            //ignore
        } else if strategy.mask.is_some()
            && field.ty.to_token_stream().to_string().as_str() == "String"
        {
            fields
                .push(quote! {.field(#ident_name, &&suanleme_common::log::mask_str(&self.#ident))});
        } else if let Some(limit) = strategy.limit {
            let limit = limit.parse::<usize>().unwrap();
            if field.ty.to_token_stream().to_string().as_str() == "String" {
                fields.push(
                    quote! {.field(#ident_name, &&format!("{}..",suanleme_common::log::limit_str(&self.#ident,#limit)))},
                );
            } else {
                fields.push(
                    quote! {.field(#ident_name, &&format!("{}..",suanleme_common::log::limit_str(&format!("{:?}",&self.#ident),#limit)))},
                );
            }
        } else {
            fields.push(quote! {.field(#ident_name, &self.#ident)});
        };
    }
    let ident_name = ident.to_string();
    let token = quote! {
        impl #generics_0 std::fmt::Debug for #ident #generics_1
        #generics_2
        {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.debug_struct(#ident_name)
                    #(#fields)*
                    .finish()
            }
        }
    };
    token.into()
}

fn get_strategy_by_attrs(attrs: &Vec<Attribute>) -> Result<StrategyAttr, syn::Error> {
    for attr in attrs {
        if let Meta::List(list) = &attr.meta {
            if let Some(segment) = list.path.segments.first() {
                if segment.ident == "strategy" {
                    return StrategyAttr::from_attr(list.tokens.clone().into());
                }
            }
        }
    }
    Ok(StrategyAttr::default())
}
