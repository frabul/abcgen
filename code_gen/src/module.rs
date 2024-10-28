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

        let actor_implementations = module_items
            .iter()
            .filter_map(|item| is_impl_of(item, actor_id))
            .collect::<Vec<_>>();

        let impl_items = actor_implementations
            .iter()
            .flat_map(|item| item.items.iter());

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
        validate_start_method(&actor_implementations, &events)?;
        // validate the shutdown method
        validate_shutdown_method(&actor_implementations)?;

        // find handler methods
        let mut methods: Vec<MessageHandlerMethod<'a>> = Vec::new();
        for item in impl_items {
            if let syn::ImplItem::Fn(m) = item {
                if m.attrs.iter().any(|a| test_attribute(a, "message_handler")) {
                    methods.push(MessageHandlerMethod::new(m)?);
                }
            }
        }

        // todo check if there is a channel_error( ... ) function ( that is used to create a channel error ) and store its return type
        // todo check if there is a an already_stopped_error( ... ) function ( that is used to create an already stopped error ) and store its return type
        // if return type from channel_error is different from return type from already_stopped_error, then return an error
        // if they are not found they need to be genereated with return type  AbcgenError

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
        let struct_name = self.actor_ident;
        let event_sender_alias = match self.events.as_ref() {
            Some(events) => {
                quote::quote! { type EventSender = tokio::sync::broadcast::Sender<#events>;               }
            }

            None => TokenStream::new(),
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
}

fn validate_start_method(
    actor_implementations: &Vec<&syn::ItemImpl>,
    events: &Option<&Ident>,
) -> Result<()> {
    let start_method = actor_implementations
        .iter()
        .flat_map(|item| item.items.iter())
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
        "Expected a start method: `fn start(&mut self, task_sender: TaskSender, event_sender: EventSender)`"
    } else {
        "Expected a start method: `fn start(&mut self, task_sender: TaskSender)`"
    };

    if start_method.len() != 1 {
        return Err(Error::new_spanned(actor_implementations[0], the_error_msg));
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

fn validate_shutdown_method(actor_implementations: &Vec<&syn::ItemImpl>) -> Result<()> {
    let the_method = actor_implementations
        .iter()
        .flat_map(|item| item.items.iter())
        .filter_map(|item| {
            if let syn::ImplItem::Fn(m) = item {
                if m.sig.ident == "shutdown" {
                    return Some(m);
                }
            }
            None
        })
        .collect::<Vec<_>>();

    let the_error_msg = "Expected a shutdown method: `fn shutdown(&mut self)`";
    if the_method.len() != 1 {
        return Err(Error::new_spanned(actor_implementations[0], the_error_msg));
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
