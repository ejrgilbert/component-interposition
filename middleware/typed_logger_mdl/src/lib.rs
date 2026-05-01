mod bindings {
    wit_bindgen::generate!({
        world: "typed-logger-mdl",
        async: true,
        generate_all
    });
}

use crate::bindings::exports::splicer::tier2::before::Guest as BeforeGuest;
use crate::bindings::splicer::common::types::{CallId, Cell, Field};

pub struct Service;

impl BeforeGuest for Service {
    async fn on_call(call: CallId, args: Vec<Field>) {
        let rendered: Vec<String> = args.iter().map(fmt_arg).collect();
        let suffix = if rendered.is_empty() {
            " -- ()".to_string()
        } else {
            format!(" -- ({})", rendered.join(", "))
        };
        println!(
            "  >> [mdl-{}#{}] before!{}",
            call.interface_name, call.function_name, suffix,
        );
    }
}

fn fmt_arg(f: &Field) -> String {
    let cell = f.tree.cells.get(f.tree.root as usize);
    let (ty, val) = match cell {
        Some(Cell::Bool(b)) => ("bool", b.to_string()),
        Some(Cell::Integer(i)) => ("int", i.to_string()),
        Some(Cell::Floating(x)) => ("float", x.to_string()),
        Some(Cell::Text(s)) => ("text", format!("{s:?}")),
        Some(Cell::Bytes(b)) => ("bytes", format!("[{}B]", b.len())),
        Some(other) => ("?", format!("{other:?}")),
        None => ("?", String::from("<missing>")),
    };
    format!("{}: {} = {}", f.name, ty, val)
}

bindings::export!( Service with_types_in bindings );
