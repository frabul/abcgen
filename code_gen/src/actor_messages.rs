use proc_macro2::TokenStream;
use syn::{Ident, Type};

use crate::MessageHandlerMethod;

pub struct MessageEnumVariant<'a> {
    name: Ident,
    parameters: Vec<(&'a Ident, &'a Type)>,
    return_type: Option<&'a Type>,
}

pub struct MessageEnum<'a> {
    pub name: Ident,
    variants: Vec<MessageEnumVariant<'a>>,
}

impl MessageEnum<'_> {
    pub fn new<'a>(
        name: Ident,
        handler_methods: &[MessageHandlerMethod<'a>],
    ) -> syn::Result<MessageEnum<'a>> {
        let mut message_variants = Vec::<MessageEnumVariant>::new();

        for handler_method in handler_methods {
            let message_variant = MessageEnumVariant::new(handler_method)?;
            message_variants.push(message_variant);
        }
        Ok(MessageEnum {
            name,
            variants: message_variants,
        })
    }

    pub fn generate(&self) -> syn::Result<TokenStream> {
        let enum_name = &self.name;
        let message_variants: Vec<_> = self.variants.iter().map(|m| m.generate()).collect();
        for message_variant in message_variants.iter() {
            if message_variant.is_err() {
                return message_variant.clone();
            }
        }
        let message_variants = message_variants.into_iter().map(|elem| elem.unwrap());
        let message_enum = quote::quote! {
            #[derive(Debug)]
            pub enum #enum_name {
                #(#message_variants),*
            }
        };
        Ok(message_enum)
    }
}

impl MessageEnumVariant<'_> {
    fn new<'a>(handler_method: &MessageHandlerMethod<'a>) -> syn::Result<MessageEnumVariant<'a>> {
        let parameters: Vec<(&Ident, &Type)> = handler_method.parameters.clone();
        let name = handler_method.get_name_camel_case();

        Ok(MessageEnumVariant {
            name: name.clone(),
            parameters,
            return_type: handler_method.get_return_type(),
        })
    }

    fn generate(&self) -> syn::Result<TokenStream> {
        let variant_name = &self.name;
        let parameters = self.parameters.iter().map(|(name, ty)| {
            quote::quote! { #name: #ty }
        });
        // if there is a return type T then we must add a respond_to field with the type tokio::sync::oneshot::Sender<T>
        let mut respond_to = TokenStream::new();
        if let Some(ret_type) = self.return_type {
            respond_to = quote::quote! {
                respond_to: tokio::sync::oneshot::Sender<#ret_type>
            };
        }

        let message_variant = quote::quote! {
            #variant_name {
                #(#parameters,)*
                #respond_to
            }
        };
        Ok(message_variant)
    }
}
