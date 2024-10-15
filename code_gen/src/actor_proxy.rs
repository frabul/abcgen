use proc_macro2::TokenStream;
use syn::{Ident, ReturnType};

use crate::MessageHandlerMethod;

pub struct ActorProxy<'a> {
    pub name: Ident,
    methods: Vec<MessageHandlerMethod<'a>>,
    message_enum: Ident,
    events_enum: Option<&'a Ident>,
}

impl ActorProxy<'_> {
    pub fn new<'a>(
        name: Ident,
        message_enum: Ident,
        events_enum: Option<&'a Ident>,
        methods: &[MessageHandlerMethod<'a>],
    ) -> ActorProxy<'a> {
        ActorProxy {
            name,
            methods: methods.to_vec(),
            message_enum,
            events_enum,
        }
    }

    pub fn generate(&self) -> TokenStream {
        let struct_name = &self.name;
        let message_enum_name = &self.message_enum;
        let methods = self
            .methods
            .iter()
            .map(|m| self.generate_method(m))
            .collect::<Vec<_>>();
        let events_enum: Vec<_> = self.events_enum.iter().collect();

        // -- def --
        let struct_def = quote::quote! {
            pub struct #struct_name {
                message_sender: tokio::sync::mpsc::Sender<#message_enum_name>,
                #(events: tokio::sync::broadcast::Receiver<#events_enum>,)*
                stop_signal: std::option::Option<tokio::sync::oneshot::Sender<()>>,
            }
        };

        // -- impl --
        let struct_impl = quote::quote! {
            impl #struct_name {
                pub fn is_running(&self) -> bool {
                    match self.stop_signal.as_ref() {
                        Some(s) => !s.is_closed(),
                        None => false,
                    }
                }

                pub fn stop(&mut self) -> Result<(), AbcgenError> {
                    match self.stop_signal.take() {
                        Some(tx) => tx.send(()).map_err(|_e: ()| AbcgenError::AlreadyStopped),
                        None => Err(AbcgenError::AlreadyStopped),
                    }
                }

                pub async fn stop_and_wait(&mut self) -> Result<(), AbcgenError> {
                    self.stop()?;
                    while self.is_running() {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    Ok(())
                }

                #(pub fn get_events(&self) -> tokio::sync::broadcast::Receiver<#events_enum> {
                    self.events.resubscribe()
                })*

                //---- message sender methods ----
                #(#methods)*

            }
        };

        quote::quote! {
            #struct_def
            #struct_impl
        }
    }

    fn generate_method(&self, handler: &MessageHandlerMethod) -> TokenStream {
        let fn_name = handler.get_name_snake_case();
        let msg_name = handler.get_name_camel_case();
        let message_enum_name = &self.message_enum;
        let parameters = handler
            .parameters
            .iter()
            .map(|(name, ty)| {
                quote::quote! {
                    #name: #ty
                }
            })
            .collect::<Vec<_>>();

        let parameters_names = handler
            .parameters
            .iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        if let ReturnType::Type(_, ref return_type) = handler.original.sig.output {
            quote::quote! {
                pub async fn #fn_name(&self, #(#parameters),*) -> Result<#return_type, AbcgenError> {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    let msg = #message_enum_name::#msg_name { #(#parameters_names),* , respond_to: tx };
                    let send_res = self.message_sender.send(msg).await;
                    match send_res {
                        Ok(_) => rx.await.map_err(|e| AbcgenError::ChannelError(Box::new(e))),
                        Err(e) => Err(AbcgenError::ChannelError(Box::new(e))),
                    }
                }
            }
        } else {
            quote::quote! {
                pub async fn #fn_name(&self, #(#parameters),*) -> Result<(), AbcgenError> {
                    let msg = #message_enum_name::#msg_name { #(#parameters_names),* };
                    let send_res = self.message_sender.send(msg).await.map_err(|e| AbcgenError::ChannelError(Box::new(e)));
                    send_res
                }
            }
        }
    }
}
