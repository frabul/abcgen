use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::{MessageEnum, MessageHandlerMethod};

/// Actor code generator
pub struct Actor<'a> {
    ident: &'a syn::Ident,
    proxy_ident: &'a Ident,
    generic_params: Option<&'a syn::Generics>,
    handler_methods: Vec<&'a MessageHandlerMethod<'a>>,
    message_enum: &'a MessageEnum<'a>,
    events: Option<&'a Ident>,
    msg_chan_size: usize,
    task_chan_size: usize,
}

impl<'a> Actor<'a> {
    pub fn generate(&self) -> TokenStream {
        let Self {
            ident: struct_name,
            proxy_ident ,
            message_enum,
            events,
            msg_chan_size,
            task_chan_size,
            generic_params,
            ..
        } = self;

        let messages_enum_name = &message_enum.name;
        let messages_enum_name = quote! { #messages_enum_name #(#generic_params) };
        let message_dispatcher_method = self.generate_dispatcher_method();
        
        let proxy = quote! { #proxy_ident #(#generic_params)} ;

        let (events1, events2, events3) = match events {
            Some(events) => (
                quote::quote! { let (event_sender, _) = tokio::sync::broadcast::channel::<#events>(20);
                let event_sender_clone = event_sender.clone();                            },
                quote::quote! { ,event_sender_clone                                                       },
                quote::quote! { events: event_sender,                                                     },
            ),
            None => (TokenStream::new(), TokenStream::new(), TokenStream::new()),
        };

        quote::quote! {
            impl #(#generic_params) #struct_name #(#generic_params){
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

                /// Helper function to send a task to be invoked in the actor loop
                fn invoke(sender: &tokio::sync::mpsc::Sender<Task<#struct_name>>, task: Task<#struct_name>) -> Result<(), AbcgenError> {
                    sender.try_send(task)
                          .map_err(|e| AbcgenError::ChannelError(Box::new(e)))
                }
                //fn invoke_fn(sender: &tokio::sync::mpsc::Sender<Task<#struct_name>>, f: fn(&mut #struct_name) -> PinnedFuture<()> + Send>) -> Result<(), AbcgenError> {
                //    Self::invoke_task(sender, Box::new(move |actor| f(actor)))
                //}

            }

        }
    }

    fn generate_dispatcher_method(&self) -> TokenStream {
        let patterns = self
            .handler_methods
            .iter()
            .map(|m| self.generate_message_handler_case(m));
        let enum_name = &self.message_enum.name;
        quote::quote! {
            async fn dispatch(&mut self, message: #enum_name) {
                match message {
                    #(#patterns)*
                }
            }
        }
    }

    fn generate_message_handler_case(&self, method: &MessageHandlerMethod) -> TokenStream {
        let method_name = method.get_name_snake_case();
        let variant_name = method.get_name_camel_case();
        let enum_name = &self.message_enum.name;

        let method_params_names: Vec<_> = method.get_parameter_names();
        if method.has_return_type() {
            quote::quote! {
                #enum_name::#variant_name { #(#method_params_names,)* respond_to } => {
                    let result = self.#method_name(#(#method_params_names),*).await;
                    respond_to.send(result).unwrap();
                }
            }
        } else {
            quote::quote! {
                #enum_name::#variant_name { #(#method_params_names),* } => {
                    self.#method_name(#(#method_params_names),*).await;
                }
            }
        }
    }
}
