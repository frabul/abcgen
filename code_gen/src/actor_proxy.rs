use proc_macro2::TokenStream;
use syn::{Ident, ReturnType, Type, TypePath};

use crate::{utils::type_path_from_generic_argument, MessageHandlerMethod};

pub struct ActorProxy<'a> {
    pub name: Ident,
    methods: Vec<MessageHandlerMethod<'a>>,
    message_enum: Ident,
    events_enum: Option<&'a Ident>,
    convertible_errors: Vec<&'a TypePath>,
}

impl ActorProxy<'_> {
    pub fn new<'a>(
        name: Ident,
        message_enum: Ident,
        events_enum: Option<&'a Ident>,
        methods: &[MessageHandlerMethod<'a>],
        convertible_errors: Vec<&'a TypePath>,
    ) -> ActorProxy<'a> {
        ActorProxy {
            name,
            methods: methods.to_vec(),
            message_enum,
            events_enum,
            convertible_errors,
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
        let struct_def: TokenStream = quote::quote! {
            pub struct #struct_name {
                message_sender: tokio::sync::mpsc::Sender<#message_enum_name>,
                #(events: tokio::sync::broadcast::Sender<#events_enum>,)*
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
                        Some(tx) => tx.send(()).map_err(|_e: ()| AbcgenError::ActorShutdown),
                        None => Err(AbcgenError::ActorShutdown),
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
                    self.events.subscribe()
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
            // if the return type is Result<U,V> and V implements From<AbcgenError> then instead of
            // returning Result<Result<U,V>,AbcgenError> we return Result<U,V> directly
            let mut can_be_converted = false;
            if let Type::Path(ref path) = **return_type {
                can_be_converted = self.check_if_can_be_converted(path);
            }
            if can_be_converted {
                quote::quote! {
                    pub async fn #fn_name(&self, #(#parameters),*) -> #return_type {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let msg = #message_enum_name::#msg_name { #(#parameters_names,)* respond_to: tx };
                        let send_res = self.message_sender.send(msg).await;
                        match send_res {
                            Ok(_) => rx.await.unwrap_or_else(|e| Err(AbcgenError::ActorShutdown.into())),
                            Err(e) => Err(AbcgenError::ActorShutdown.into()),
                        }
                    }
                }
            } else {
                quote::quote! {
                    pub async fn #fn_name(&self, #(#parameters),*) -> Result<#return_type, AbcgenError> {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let msg = #message_enum_name::#msg_name { #(#parameters_names,)* respond_to: tx };
                        let send_res = self.message_sender.send(msg).await;
                        match send_res {
                            Ok(_) => rx.await.map_err(|e| AbcgenError::ActorShutdown),
                            Err(e) => Err(AbcgenError::ActorShutdown),
                        }
                    }
                }
            }
        } else {
            quote::quote! {
                pub async fn #fn_name(&self, #(#parameters),*) -> Result<(), AbcgenError> {
                    let msg = #message_enum_name::#msg_name { #(#parameters_names),* };
                    let send_res = self.message_sender.send(msg).await.map_err(|e| AbcgenError::ActorShutdown );
                    send_res
                }
            }
        }
    }

    fn check_if_can_be_converted(&self, path: &TypePath) -> bool {
        // path her is something like Result<U,V>
        let mut can_be_converted = false;
        if let Some(segment) = path.path.segments.last() {
            if segment.ident == "Result" {
                match &segment.arguments {
                    syn::PathArguments::None => {}
                    syn::PathArguments::AngleBracketed(args) => {
                        let err_type = if args.args.len() == 2 {
                            type_path_from_generic_argument(&args.args[1])
                        } else if args.args.len() == 1 {
                            type_path_from_generic_argument(&args.args[0])
                        } else {
                            None
                        };
                        if let Some(err_type) = err_type {
                            for ty in self.convertible_errors.iter() {
                                if crate::utils::compare_type_path(&err_type, ty) {
                                    can_be_converted = true;
                                    break;
                                }
                            }
                        }
                    }
                    syn::PathArguments::Parenthesized(_) => {}
                }
            }
        }
        can_be_converted
    }
}
