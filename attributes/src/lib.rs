use proc_macro::TokenStream;
use syn::parse_macro_input;

use ab_code_gen::*;

/// The `actor_module` attribute is used to mark a module that contains the actor definition.
/// It will generate the necessary code to implement the actor pattern, that is:
/// - a proxy object that implements all the methods that are marked with the `message_handler` attribute
/// - a message enum that contains all the messages that the actor can receive
/// - a `run` method for that is going to start the actor and return the proxy object
/// - all the needed logic for the actor
#[proc_macro_attribute]
pub fn actor_module(args: TokenStream, input: TokenStream) -> TokenStream {
    let _ = args;
    let _ = input;
    let mut input = parse_macro_input!(input as syn::ItemMod);
    let module = ActorModule::new(&input);
    if let Err(e) = module {
        return e.to_compile_error().into();
    }

    let res = module.unwrap().generate();
    if let Err(e) = res {
        return e.to_compile_error().into();
    }
    let code_to_add = res.unwrap();
    input
        .content
        .as_mut()
        .unwrap()
        .1
        .push(syn::Item::Verbatim(code_to_add));
    quote::quote! {#input}.into()
}

/// This attribute is used to mark the struct or enum that is going to be the actor.
#[proc_macro_attribute]
pub fn actor(args: TokenStream, input: TokenStream) -> TokenStream {
    let _ = args;
    let _ = input;
    input
}

/// This attribute is used to mark the enum that defines the events that the actor can signal.
#[proc_macro_attribute]
pub fn events(args: TokenStream, input: TokenStream) -> TokenStream {
    let _ = args;
    let _ = input;
    input
}

/// This attribute is used to mark the methods that are going to handle the messages that the actor can receive.
#[proc_macro_attribute]
pub fn message_handler(args: TokenStream, input: TokenStream) -> TokenStream {
    let _ = args;
    let _ = input;
    input
}
