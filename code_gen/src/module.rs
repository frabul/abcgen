use proc_macro2::TokenStream;
use syn::{Error, Ident, Item, ItemMod, Result, Type};

use crate::{ActorProxy, MessageEnum, MessageHandlerMethod};

pub struct ActorModule<'a> {
    actor_ident: &'a Ident,
    handler_methods: Vec<MessageHandlerMethod<'a>>,
    events: Option<&'a Ident>,
    message_enum: MessageEnum<'a>,
    proxy: ActorProxy<'a>,
}

impl ActorModule<'_> {
    pub fn new<'a>(module: &'a ItemMod) -> Result<ActorModule<'a>> {
        let module_items = &module
            .content
            .as_ref()
            .ok_or_else(|| Error::new_spanned(module, "Expected module to have content"))?
            .1;
        // find the actor
        let actors = module_items
            .iter()
            .filter(|item| is_actor(item))
            .collect::<Vec<_>>();

        if actors.len() != 1 {
            return Err(Error::new_spanned(
                module,
                "Expected exactly one actor that is one struct or enum with #[actor] attribute",
            ));
        }
        let actor_id = match actors[0] {
            Item::Struct(it) => &it.ident,
            Item::Enum(it) => &it.ident,
            _ => unreachable!(),
        };

        // find handler methods
        let actor_implementations = module_items
            .iter()
            .filter_map(|item| is_impl_of(item, actor_id))
            .collect::<Vec<_>>();

        let impl_items = actor_implementations
            .iter()
            .flat_map(|item| item.items.iter());
        let mut methods: Vec<MessageHandlerMethod<'a>> = Vec::new();
        for item in impl_items {
            if let syn::ImplItem::Fn(m) = item {
                if m.attrs.iter().any(|a| test_attribute(a, "message_handler")) {
                    methods.push(MessageHandlerMethod::new(m)?);
                }
            }
        }

        // check if there are any events
        let events = extract_events_enum(module_items)?;
        if events.len() > 1 {
            return Err(Error::new_spanned(
                events.last().unwrap(),
                "Expected at most one events enum",
            ));
        }
        let events = events.into_iter().next();
        // build generators
        let msg_generator = MessageEnum::new(quote::format_ident!("{actor_id}Message"), &methods)?;
        let proxy = ActorProxy::new(
            quote::format_ident!("{actor_id}Proxy"),
            msg_generator.name.clone(),
            events,
            &methods,
        );

        Ok(ActorModule {
            actor_ident: actor_id,
            handler_methods: methods.clone(),
            events: events.into_iter().next(),
            message_enum: msg_generator,
            proxy,
        })
    }

    pub fn generate(&self) -> Result<TokenStream> {
        let proxy = &self.proxy.name;
        let message_dispatcher_method = self.generate_dispatcher_method();
        let messages_enum_name = &self.message_enum.name;
        let struct_name = self.actor_ident;
        let (events1, events2, events3, event_sender_alias) = match self.events.as_ref() {
            Some(events) => (
                quote::quote! { let (event_sender, _) = tokio::sync::broadcast::channel::<#events>(20);  
                                let event_sender_clone = event_sender.clone();                            }, 
                quote::quote! { ,event_sender_clone                                                       },
                quote::quote! { events: event_sender,                                                     },
                quote::quote! { type EventSender = tokio::sync::broadcast::Sender<#events>;               },
            ),
            None => (
                TokenStream::new(),
                TokenStream::new(),
                TokenStream::new(),
                TokenStream::new(),
            ),
        };
        let actor_impl = quote::quote! {
            impl #struct_name {
                pub fn run(self) -> #proxy {
                    let (msg_sender, mut msg_receiver) = tokio::sync::mpsc::channel(20);
                    #events1
                    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel::<()>();
                    let (task_sender, mut task_receiver) = tokio::sync::mpsc::channel::<Task<#struct_name>>(20);
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

        };

        let message_enum_code = self.message_enum.generate()?;
        let proxy_code = self.proxy.generate();
        let code = quote::quote! {

            #event_sender_alias
            pub type TaskSender = tokio::sync::mpsc::Sender<Task<#struct_name>>;

            #message_enum_code
            #proxy_code
            #actor_impl
        };

        Ok(code)
    }

    pub fn generate_dispatcher_method(&self) -> TokenStream {
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
    pub fn generate_message_handler_case(&self, method: &MessageHandlerMethod) -> TokenStream {
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

fn test_attribute(attr: &syn::Attribute, expected: &str) -> bool {
    attr.path().segments.last().unwrap().ident == expected
}

fn is_actor(item: &Item) -> bool {
    let attribututes = match item {
        Item::Struct(it) => &it.attrs,
        Item::Enum(it) => &it.attrs,
        _ => return false,
    };
    attribututes
        .iter()
        .any(|attr| test_attribute(attr, "actor"))
}

fn is_impl_of<'a>(item: &'a Item, id: &'a Ident) -> Option<&'a syn::ItemImpl> {
    if let Item::Impl(item_impl) = item {
        if let Type::Path(tp) = item_impl.self_ty.as_ref() {
            if tp.path.segments.last().unwrap().ident == *id {
                return Some(item_impl);
            }
        }
    }
    None
}

fn extract_events_enum(items: &[syn::Item]) -> Result<Vec<&Ident>> {
    let events: Vec<_> = items
        .iter()
        .filter_map(|i| {
            match i {
                Item::Enum(e) => {
                    if e.attrs.iter().any(|a| test_attribute(a, "events")) {
                        return Some(&e.ident);
                    }
                }
                Item::Type(t) => {
                    if t.attrs.iter().any(|a| test_attribute(a, "events")) {
                        return Some(&t.ident);
                    }
                }
                Item::Struct(s) => {
                    if s.attrs.iter().any(|a| test_attribute(a, "events")) {
                        return Some(&s.ident);
                    }
                }
                _ => {}
            };
            None
        })
        .collect();
    Ok(events)
}
