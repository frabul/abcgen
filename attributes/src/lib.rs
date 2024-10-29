use proc_macro::TokenStream;
use syn::parse_macro_input;

use ab_code_gen::Config;
use ab_code_gen::*;

/// The `actor_module` attribute is used to mark a module that contains the actor definition.
/// It will generate the necessary code to implement the actor pattern, that is:
/// - a proxy object that implements all the methods that are marked with the `message_handler` attribute
/// - a message enum that contains all the messages that the actor can receive
/// - a `run` method for that is going to start the actor and return the proxy object
/// - all the needed logic for the actor
///
/// It support following arguments as key-value pairs:
/// - `channels_size`: the size of the channels used to communicate with the actor. Default is 20.
#[proc_macro_attribute]
pub fn actor_module(args: TokenStream, input: TokenStream) -> TokenStream {
    let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
    let args: syn::punctuated::Punctuated<syn::Expr, syn::token::Comma> =
        parse_macro_input!(args with parser);
    let config = get_config(args);
    if let Err(e) = config {
        return e.to_compile_error().into();
    }
    let config = config.unwrap();
    let mut input = parse_macro_input!(input as syn::ItemMod);
    let module = ActorModule::new(&input, &config);
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

fn get_config(
    args: syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
) -> syn::Result<Config> {
    let mut config = Config { channels_size: 20 };

    for arg in args {
        match arg {
            syn::Expr::Assign(expr_assign) => match expr_assign.left.as_ref() {
                syn::Expr::Path(expr_path) => {
                    let ident = expr_path.path.get_ident().unwrap();
                    match ident.to_string().as_str() {
                        "channels_size" => {
                            config.channels_size = parse_usize(&expr_assign.right)?;
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(ident, "Unsupported argument"));
                        }
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        expr_assign.left,
                        "Invalid argument",
                    ))
                }
            },
            _ => return Err(syn::Error::new_spanned(arg, "Invalid argument")),
        }
    }
    Ok(config)
}

/// This attribute is used to mark the struct or enum that is going to be the actor.
#[proc_macro_attribute]
pub fn actor(args: TokenStream, input: TokenStream) -> TokenStream {
    let _ = args;
    let _ = input;
    input
}

/// This attribute is used to mark the enum that defines the events that the actor can signal.
/// It can be applied to
/// - structs
/// - enums
/// - type aliases
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

fn parse_usize(expr: &syn::Expr) -> syn::Result<usize> {
    match expr {
        syn::Expr::Lit(expr_lit) => match &expr_lit.lit {
            syn::Lit::Int(lit_int) => {
                let value = lit_int.base10_parse::<usize>();
                match value {
                    Ok(v) => Ok(v),
                    Err(_) => Err(syn::Error::new_spanned(lit_int, "Invalid value, literal usize expected.")),
                }
            }
            _ => Err(syn::Error::new_spanned(expr_lit, "Invalid value")),
        },
        _ => Err(syn::Error::new_spanned(expr, "Invalid expression, `key=value` expected.")),
    }
}
