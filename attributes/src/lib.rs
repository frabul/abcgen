use proc_macro::TokenStream;
use syn::parse_macro_input;

use ab_code_gen::*;

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

#[proc_macro_attribute]
pub fn actor(args: TokenStream, input: TokenStream) -> TokenStream {
    let _ = args;
    let _ = input;
    input
}
#[proc_macro_attribute]
pub fn events(args: TokenStream, input: TokenStream) -> TokenStream {
    let _ = args;
    let _ = input;
    input
}
#[proc_macro_attribute]
pub fn message_handler(args: TokenStream, input: TokenStream) -> TokenStream {
    let _ = args;
    let _ = input;
    input
}
