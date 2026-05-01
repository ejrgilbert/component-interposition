mod bindings {
    wit_bindgen::generate!({
        world: "typed-logger-mdl",
        async: true,
        generate_all
    });
}

use crate::bindings::exports::splicer::tier2::before::Guest as BeforeGuest;
use crate::bindings::exports::splicer::tier2::after::Guest as AfterGuest;
use crate::bindings::splicer::common::types::{CallId, Cell, Field, FieldTree};

pub struct Service;

impl BeforeGuest for Service {
    async fn on_call(call: CallId, args: Vec<Field>) {
        let rendered: Vec<String> = args.iter().map(fmt_arg).collect();
        let suffix = if rendered.is_empty() {
            " ()".to_string()
        } else {
            format!(" ({})", rendered.join(", "))
        };
        println!(
            "  >> [mdl-{}#{}]{}",
            call.interface_name, call.function_name, suffix,
        );
    }
}

impl AfterGuest for Service {
    async fn on_return(call: CallId, result: Option<FieldTree>) {
        let suffix = if let Some(tree) = &result {
            format!(" --> {}", fmt_res(tree))
        } else {
            " --> ()".to_string()
        };
        println!(
            "  >> [mdl-{}#{}]{}",
            call.interface_name, call.function_name, suffix,
        );
    }
}

fn fmt_arg(f: &Field) -> String {
    let cell = f.tree.cells.get(f.tree.root as usize);
    let (ty, val) = cell_to_str(cell);
    format!("{}: {ty} = {val}", f.name)
}

fn fmt_res(tree: &FieldTree) -> String {
    let cell = tree.cells.get(tree.root as usize);
    let (ty, val) = cell_to_str(cell);
    format!("({ty}: {val})")
}

fn cell_to_str(cell: Option<&Cell>) -> (String, String) {
    match cell {
        Some(Cell::Bool(b)) => ("bool".to_string(), b.to_string()),
        Some(Cell::Integer(i)) => ("int".to_string(), i.to_string()),
        Some(Cell::Floating(x)) => ("float".to_string(), x.to_string()),
        Some(Cell::Text(s)) => ("text".to_string(), format!("{s:?}")),
        Some(Cell::Bytes(b)) => ("bytes".to_string(), format!("[{}B]", b.len())),
        Some(other) => ("?".to_string(), format!("{other:?}")),
        None => ("?".to_string(), String::from("<missing>")),
    }
}

bindings::export!( Service with_types_in bindings );
