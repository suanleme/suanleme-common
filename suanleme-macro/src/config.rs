use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Data, DeriveInput};

pub fn hot_config(item: TokenStream) -> TokenStream {
    let org_item = parse_macro_input!(item as DeriveInput);
    let ident = &org_item.ident;
    let hot_config_sender = syn::Ident::new(&format!("Hot{}Sender", ident), ident.span());
    let hot_config_receiver = syn::Ident::new(&format!("Hot{}Receiver", ident), ident.span());

    let Data::Struct(data_struct) = &org_item.data else {
        return syn::Error::new_spanned(org_item.to_token_stream(), "builder must label to struct")
            .into_compile_error()
            .into();
    };
    let fields = data_struct.fields.iter().fold(vec![], |mut vec, e| {
        vec.push(e.to_token_stream());
        vec
    });
    let de = quote! (

        #[derive(serde::Deserialize,Debug)]
        pub struct #ident {
            #[serde(skip_serializing)]
            #[serde(skip_deserializing)]
            _hot_config_sender_ : Option<tokio::sync::mpsc::UnboundedSender<(#hot_config_sender,tokio::sync::oneshot::Sender<#hot_config_receiver>)>>,
            #(#fields),*
        }

        pub enum #hot_config_sender {
            GET,
            CHANGE(#ident),
        }

        pub enum #hot_config_receiver {
            GET(std::sync::Arc<#ident>),
            CHANGE,
        }

        impl suanleme_common::config::HotConfig for #ident {
            fn build_hot_config(
                &mut self,
                ident : #ident,
                mut listener: tokio::sync::mpsc::Receiver<nacos_sdk::api::config::ConfigResponse>)
                -> Result<(),suanleme_common::error::BoxError> {
                let (sender, mut receive) =
                tokio::sync::mpsc::unbounded_channel::<(#hot_config_sender, tokio::sync::oneshot::Sender<#hot_config_receiver>)>();
                tokio::spawn(
                    async move {
                        let mut cache = std::sync::Arc::new(ident);
                        loop {
                            while let Some((msg,sender)) = receive.recv().await {
                                match msg {
                                    #hot_config_sender::GET => {
                                        let _ = sender.send(#hot_config_receiver::GET(cache.clone()));
                                    },
                                    #hot_config_sender::CHANGE(ident) => {
                                        cache = std::sync::Arc::new(ident);
                                        let _ = sender.send(#hot_config_receiver::CHANGE);
                                    }
                                }
                            }
                        }
                    }
                );
                let sender_clone = sender.clone();
                tokio::spawn(
                    async move {
                        let sender_clone = sender_clone;
                        while let Some(config_response) = listener.recv().await {
                            let Ok(ident) = suanleme_common::nacos::NacosConfiguration::config_build(config_response) else {
                                tracing::error!("config_build error!");
                                continue;
                            };
                            let (sender , receive) = tokio::sync::oneshot::channel();
                            let _ = sender_clone.send((#hot_config_sender::CHANGE(ident),sender));
                            if receive.await.is_err() {
                                tracing::error!("receive error!");
                            }
                        }
                    }
                );
                let _ = self._hot_config_sender_.insert(sender);
                Ok(())
            }
            async fn get_hot_config(&self) -> Option<std::sync::Arc<#ident>> {
                let (sender , receive) = tokio::sync::oneshot::channel();
                let _ = self._hot_config_sender_.clone()?.send((#hot_config_sender::GET,sender));
                let Ok(#hot_config_receiver::GET(ident)) = receive.await else {
                    panic!("impossibility !!!");
                };
                Some(ident)
            }
        }
    );
    eprintln!("{:#?}", de.to_string());
    de.into()
}
