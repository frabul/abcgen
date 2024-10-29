use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

use crate::{ActorModule, MessageHandlerMethod};

/// Actor code generator
pub struct Actor<'a> {
    pub(crate) ident: &'a syn::Ident,
    pub(crate) generic_params: Option<&'a syn::Generics>,
    pub(crate) handler_methods: Vec<MessageHandlerMethod<'a>>,
    pub(crate) msg_chan_size: usize,
    pub(crate) task_chan_size: usize,
    pub(crate) events_chan_size: usize,
}

impl<'a> Actor<'a> {
    pub fn generate(&self, module: &ActorModule) -> TokenStream {
        let Self {
            ident: struct_name,
            msg_chan_size,
            task_chan_size,
            events_chan_size,
            generic_params,
            ..
        } = self;
        let proxy_ident = &module.proxy.name;
        let events = module.events.as_ref();
        let messages_enum_name = &module.message_enum.name.to_token_stream();
        let message_dispatcher_method = self.generate_dispatcher_method(messages_enum_name);
        let generic_params: Vec<_> = generic_params.iter().collect();
        let proxy = quote! { #proxy_ident #(#generic_params)*};

        let (events1, events2, events3) = match events {
            Some(events) => (
                quote::quote! { let (event_sender, _) = tokio::sync::broadcast::channel::<#events>(#events_chan_size);
                let event_sender_clone = event_sender.clone();                            },
                quote::quote! { ,event_sender_clone                                                       },
                quote::quote! { events: event_sender,                                                     },
            ),
            None => (TokenStream::new(), TokenStream::new(), TokenStream::new()),
        };

        quote::quote! {
            impl #(#generic_params)* #struct_name #(#generic_params)*{
                pub fn run(self) -> #proxy {
                    let (msg_sender, mut msg_receiver) = tokio::sync::mpsc::channel(#msg_chan_size);
                    #events1
                    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel::<()>();
                    let (task_sender, mut task_receiver) = tokio::sync::mpsc::channel::<Task<#struct_name>>(#task_chan_size);
                    tokio::spawn(async move {
                        let mut actor = self;
                        actor.start(task_sender  #events2 ).await;
                        tokio::select! {
                            _ = actor.select_receivers(&mut msg_receiver, &mut task_receiver) => { log::debug!("(abcgen) all proxies dropped"); }  // all proxies were dropped => shutdown
                            _ = stop_receiver => { log::debug!("(abcgen) stop signal received"); } // stop signal received => shutdown
                        }
                        // we get here when the actor is done
                        actor.shutdown().await;
                    });

                    // build the proxy
                    let proxy = #proxy {
                        message_sender: msg_sender,
                        stop_signal: Some(stop_sender),
                        #events3
                    };

                    proxy
                }

                async fn select_receivers(
                    &mut self,
                    msg_receiver: &mut tokio::sync::mpsc::Receiver<#messages_enum_name>,
                    task_receiver: &mut tokio::sync::mpsc::Receiver<Task<#struct_name>>,
                ) {
                    loop {
                        tokio::select! {
                            msg = msg_receiver.recv() => {
                                match msg {
                                    Some(msg) => { self.dispatch(msg).await; }
                                    None => { break; } // channel closed => shutdown
                                }
                            },
                            task = task_receiver.recv() => {
                                if let Some(task) = task {
                                    task(self).await;
                                }
                            }
                        }
                    }
                }

                #message_dispatcher_method
            }

        }
    }

    fn generate_dispatcher_method(&self, messages_id: &TokenStream) -> TokenStream {
        let patterns = self
            .handler_methods
            .iter()
            .map(|m| self.generate_message_handler_case(m, messages_id));

        quote::quote! {
            async fn dispatch(&mut self, message: #messages_id) {
                match message {
                    #(#patterns)*
                }
            }
        }
    }

    fn generate_message_handler_case(
        &self,
        method: &MessageHandlerMethod,
        messages_id: &TokenStream,
    ) -> TokenStream {
        let method_name = method.get_name_snake_case();
        let variant_name = method.get_name_camel_case();

        let method_params_names: Vec<_> = method.get_parameter_names();
        if method.has_return_type() {
            quote::quote! {
                #messages_id::#variant_name { #(#method_params_names,)* respond_to } => {
                    let result = self.#method_name(#(#method_params_names),*).await;
                    respond_to.send(result).unwrap();
                }
            }
        } else {
            quote::quote! {
                #messages_id::#variant_name { #(#method_params_names),* } => {
                    self.#method_name(#(#method_params_names),*).await;
                }
            }
        }
    }
}
