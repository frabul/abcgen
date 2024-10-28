use quote::ToTokens;
use syn::TypePath;

pub fn compare_type_path(tp1: &TypePath, tp2: &TypePath) -> bool {
    let mut iter1 = tp1.path.segments.iter().rev();
    let mut iter2 = tp2.path.segments.iter().rev();
    let mut first_iter = true;
    loop {
        let seg1 = iter1.next();
        let seg2 = iter2.next();

        if seg1.is_none() || seg2.is_none() {
            break !first_iter; // it is ok if not first iter
        }
        first_iter = false;
        let seg1 = seg1.unwrap();
        let seg2 = seg2.unwrap();
        if seg1.ident != seg2.ident {
            break false;
        }

        // naive comparison of arguments by string
        let ts1 = seg1.arguments.to_token_stream().to_string();
        let ts2 = seg2.arguments.to_token_stream().to_string();

        if ts1 != ts2 {
            break false;
        }
    }
}

pub fn type_path_from_type(ty: &syn::Type) -> Option<&TypePath> {
    if let syn::Type::Path(tp) = ty {
        Some(tp)
    } else {
        None
    }
}

pub fn type_path_from_generic_argument(ga: &syn::GenericArgument) -> Option<&TypePath> {
    if let syn::GenericArgument::Type(ty) = ga {
        type_path_from_type(ty)
    } else {
        None
    }
}
