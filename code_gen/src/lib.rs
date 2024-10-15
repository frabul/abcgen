mod actor_messages;
mod module;
mod actor_proxy;

pub use actor_messages::*; 
pub use module::*;
pub use actor_proxy::*;

use convert_case::Casing;
use syn::ReturnType;



#[derive(Clone)]
pub struct MessageHandlerMethod<'a> {
    original: &'a syn::ImplItemFn,
    parameters: Vec<(&'a syn::Ident, &'a syn::Type)>,
}

impl<'b> MessageHandlerMethod<'b> {
    pub fn new(method: &syn::ImplItemFn) -> syn::Result<MessageHandlerMethod> {
        // validate the method signature
        let error = Err(syn::Error::new_spanned(
            method,
            "Message handler methods must have at least one parameter (&self)",
        ));
        if method.sig.inputs.is_empty() {
            return error;
        }
        if let syn::FnArg::Receiver(rec) = method.sig.inputs.first().unwrap() {
            if rec.reference.is_none() {
                return error;
            }
        } else {
            return error;
        }

        let parameters = method
            .sig
            .inputs
            .iter()
            .filter_map(|i| {
                if let syn::FnArg::Typed(t) = i {
                    if let syn::Pat::Ident(p) = t.pat.as_ref() {
                        return Some((&p.ident, t.ty.as_ref()));
                    }
                }
                None
            })
            .collect();

        Ok(MessageHandlerMethod {
            original: method,
            parameters,
        })
    }

    pub fn get_name_snake_case(&self) -> &syn::Ident {
        &self.original.sig.ident
    }

    pub fn get_name_camel_case(&self) -> syn::Ident {
        let name = self.get_name_snake_case();
        syn::Ident::new(
            &name.to_string().to_case(convert_case::Case::UpperCamel),
            name.span(),
        )
    }

    pub fn has_return_type(&self) -> bool {
        matches!(self.original.sig.output, ReturnType::Type(_, _))
    }

    pub fn get_return_type(&self) -> Option<&'b syn::Type> {
        match &self.original.sig.output {
            ReturnType::Type(_, ty) => Some(ty),
            _ => None,
        }
    }
    pub fn get_parameter_names(&self) -> Vec<&syn::Ident> {
        self.parameters.iter().map(|(name, _)| *name).collect()
    }
}
