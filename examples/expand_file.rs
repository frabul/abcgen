use std::{io::{self, Write}, thread::spawn};

use log::warn;
use syn::*;

fn main() {
    let content = std::fs::read_to_string("examples/print_stuff.rs").unwrap();
    let mut ast = syn::parse_file(&content).unwrap();
    // search for the module
    let module = ast
        .items
        .iter_mut()
        .find_map(|item| match item {
            Item::Mod(module) => Some(module),
            _ => None,
        })
        .unwrap();
    let actor_module = ab_code_gen::ActorModule::new(module).unwrap();

    let code = actor_module.generate().unwrap();

    module
        .content
        .as_mut()
        .unwrap()
        .1
        .push(syn::Item::Verbatim(code));
    let expanded_code = quote::quote! {#ast}.to_string();

    // pass to cargo fmt
    let mut child = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2018")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut child_stdin = child.stdin.take().unwrap();
    let mut child_stdout = child.stdout.take().unwrap();

    spawn( 
        move || {
            let _ = child_stdin.write_all(expanded_code.as_bytes());
        }
    );
    

    let mut output = vec![];
    std::io::copy(&mut child_stdout, &mut output).unwrap();

    let status = child.wait().unwrap();

    let res = match String::from_utf8(output) {
        Ok(out_str) => match status.code() {
            Some(0) => Ok(out_str),
            Some(2) => Err(io::Error::new(
                io::ErrorKind::Other,
                "Rustfmt parsing errors.".to_string(),
            )),
            Some(3) => {
                warn!("Rustfmt could not format some lines.");
                Ok(out_str)
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Other,
                "Internal rustfmt error".to_string(),
            )),
        },
        _ => unimplemented!("Rustfmt output is not utf8"),
    };

    std::fs::write("examples/print_stuff_expanded.rs", res.unwrap()).unwrap();
}
