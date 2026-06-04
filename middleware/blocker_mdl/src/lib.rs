mod bindings {
    wit_bindgen::generate!({
        world: "blocker-mdl",
        async: true,
        generate_all
    });
}

use std::env;
use crate::bindings::exports::splicer::tier1::gate::Guest;
use crate::bindings::splicer::common::types::CallId;

pub struct Service;

impl Guest for Service {
    async fn should_call(call: CallId) -> bool {
        // `SHOULD_BLOCK=true` means the user wants to skip downstream;
        // the new gate hook returns `true` to call, so invert.
        let should_call = !should_i_block_call();
        let decision = if should_call { "WILL NOT" } else { "WILL" };
        println!("  >> [mdl-block] i {decision} block call to {}#{}...",
                 call.interface_name, call.function_name);

        should_call
    }
}

fn should_i_block_call() -> bool {
    let var_value = env::var("SHOULD_BLOCK").unwrap_or_else(|_| "false".to_string());
    var_value.parse().unwrap_or(false)
}

bindings::export!( Service with_types_in bindings );
