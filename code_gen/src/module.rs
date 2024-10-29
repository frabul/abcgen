use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use syn::{spanned::Spanned, Error, Ident, ImplItem, Item, ItemMod, Result, Type};

use crate::{utils::type_path_from_type, Actor, ActorProxy, Config, MessageEnum, MessageHandlerMethod};

pub struct ActorModule<'a> {
    pub(crate) actor: Actor<'a>, 
    pub(crate) events: Option<&'a Ident>,
    pub(crate) message_enum: MessageEnum<'a>,
    pub(crate) proxy: ActorProxy<'a>,
}

impl ActorModule<'_> {
    pub fn new<'a>(module: &'a ItemMod, config: &'a Config) -> Result<ActorModule<'a>> {
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
                "Expected exactly one actor, that is one struct or enum with #[actor] attribute.",
            ));
        }
        let actor = actors[0];
        let actor_id = match actors[0] {
            Item::Struct(it) => &it.ident,
            Item::Enum(it) => &it.ident,
            _ => unreachable!(),
        };

        let actor_implementations_items = module_items
            .iter()
            .filter_map(|item| is_impl_of(item, actor_id))
            .flat_map(|item| item.items.iter())
            .collect::<Vec<_>>();

        // check if there are any events
        let events = extract_events_enum(module_items)?;
        if events.len() > 1 {
            return Err(Error::new_spanned(
                events.last().unwrap(),
                "Expected at most one events enum",
            ));
        }
        let events = events.into_iter().next();

        // validate the start method
        validate_start_method(&actor_implementations_items, &events, actor.span())?;
        // validate the shutdown method
        validate_shutdown_method(&actor_implementations_items, actor.span())?;

        // find handler methods
        let mut methods: Vec<MessageHandlerMethod<'a>> = Vec::new();
        for item in actor_implementations_items {
            if let syn::ImplItem::Fn(m) = item {
                if m.attrs.iter().any(|a| test_attribute(a, "message_handler")) {
                    methods.push(MessageHandlerMethod::new(m)?);
                }
            }
        }

        // todo find all of the implementations of trait From<abcgen::AbcgenError>
        let convertible_errors = find_convertible_error_types(module_items);
        let convertible_errors = convertible_errors
            .iter()
            .map(|t| type_path_from_type(t).unwrap())
            .collect::<Vec<_>>();

        // build generators
        let msg_generator = MessageEnum::new(quote::format_ident!("{actor_id}Message"), &methods)?;
        let proxy = ActorProxy::new(
            quote::format_ident!("{actor_id}Proxy"),
            msg_generator.name.clone(),
            events,
            &methods,
            convertible_errors,
        );
        // build actor
        let actor = Actor {
            ident: actor_id,
            generic_params: None,
            handler_methods: methods.clone(),
            msg_chan_size: config.channels_size,
            task_chan_size: config.channels_size,
        };
        Ok(ActorModule {
            actor, 
            events: events.into_iter().next(),
            message_enum: msg_generator,
            proxy,
        })
    }

    pub fn generate(&self) -> Result<TokenStream> {
        let struct_name = self.actor.ident;
        let event_sender_alias = match self.events.as_ref() {
            Some(events) => {
                quote::quote! { type EventSender = tokio::sync::broadcast::Sender<#events>;               }
            }

            None => TokenStream::new(),
        };

        let message_enum_code = self.message_enum.generate()?;
        let proxy_code = self.proxy.generate();
        let actor = self.actor.generate(self);

        let code = quote::quote! {
            #event_sender_alias
            pub type TaskSender = tokio::sync::mpsc::Sender<Task<#struct_name>>;
            #message_enum_code
            #actor
            #proxy_code
            //#actor_impl
        };

        Ok(code)
    }
}

fn find_convertible_error_types(module_items: &Vec<Item>) -> Vec<&Type> {
    let possible_types = [
        quote::quote! { From<::abcgen::AbcgenError> }.to_string(),
        quote::quote! { From<abcgen::AbcgenError> }.to_string(),
        quote::quote! { From<AbcgenError> }.to_string(),
    ];
    let mut res = Vec::new();
    for item in module_items {
        if let Item::Impl(impl_item) = item {
            if let Some((_, t_trait, _)) = impl_item.trait_.as_ref() {
                if possible_types
                    .iter()
                    .any(|t| *t == t_trait.to_token_stream().to_string())
                {
                    res.push(impl_item.self_ty.as_ref());
                }
            }
        }
    }
    res
}

fn validate_start_method(
    actor_implementations_items: &Vec<&ImplItem>,
    events: &Option<&Ident>,
    span: Span,
) -> Result<()> {
    let start_method = actor_implementations_items
        .iter()
        .filter_map(|item| {
            if let syn::ImplItem::Fn(m) = item {
                if m.sig.ident == "start" {
                    return Some(m);
                }
            }
            None
        })
        .collect::<Vec<_>>();

    let the_error_msg = if events.is_some() {
        "Expected a start method to be implemented for the actor: `fn start(&mut self, task_sender: TaskSender, event_sender: EventSender)`"
    } else {
        "Expected a start method to be implemented for the actor: `fn start(&mut self, task_sender: TaskSender)`"
    };

    if start_method.len() != 1 {
        return Err(Error::new(span.clone(), the_error_msg));
    }
    // start method found
    let start_method = start_method[0];
    let the_error = Error::new_spanned(start_method, the_error_msg);
    // check the receiver input
    let receiver_ok = start_method
        .sig
        .receiver()
        .is_some_and(|r| r.reference.is_some());
    if !receiver_ok {
        return Err(the_error);
    }
    // check the number of parameters and their types
    let other_inputs = start_method.sig.inputs.iter().skip(1).collect::<Vec<_>>();
    if events.is_some() {
        if other_inputs.len() != 2 {
            return Err(the_error);
        }
        if !check_argument_type(&other_inputs[1], "EventSender")
            || !check_argument_type(&other_inputs[0], "TaskSender")
        {
            return Err(the_error);
        }
    } else {
        if other_inputs.len() != 1 {
            return Err(the_error);
        }
        if !check_argument_type(&other_inputs[0], "TaskSender") {
            return Err(the_error);
        }
    }
    Ok(())
}

fn validate_shutdown_method(actor_implementations: &Vec<&ImplItem>, span: Span) -> Result<()> {
    let the_method = actor_implementations
        .iter()
        .filter_map(|item| {
            if let syn::ImplItem::Fn(m) = item {
                if m.sig.ident == "shutdown" {
                    return Some(m);
                }
            }
            None
        })
        .collect::<Vec<_>>();

    let the_error_msg =
        "Expected a shutdown method to be implemented for the actor: `fn shutdown(&mut self)`";
    if the_method.len() != 1 {
        return Err(Error::new(span, the_error_msg));
    }
    // start method found
    let the_method = the_method[0];
    let the_error = Error::new_spanned(the_method, the_error_msg);
    // check the receiver input
    let receiver_ok = the_method
        .sig
        .receiver()
        .is_some_and(|r| r.reference.is_some());
    if !receiver_ok {
        return Err(the_error);
    }
    // check the number of parameters and their types
    let other_inputs = the_method.sig.inputs.iter().skip(1).collect::<Vec<_>>();
    if !other_inputs.is_empty() {
        return Err(the_error);
    }
    Ok(())
}

fn check_argument_type(other_inputs: &syn::FnArg, expected_type: &str) -> bool {
    match other_inputs {
        syn::FnArg::Typed(t) => {
            if let Type::Path(tp) = t.ty.as_ref() {
                if tp.path.segments.last().unwrap().ident != expected_type {
                    false
                } else {
                    true
                }
            } else {
                false
            }
        }
        _ => false,
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
