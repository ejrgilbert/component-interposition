mod bindings {
    wit_bindgen::generate!({
        world: "blocker-mdl",
        async: true,
        generate_all
    });
}

use std::env;
use crate::bindings::exports::splicer::tier1::blocking::Guest;
use crate::bindings::splicer::common::types::CallId;

pub struct Service;

impl Guest for Service {
    async fn should_block(call: CallId) -> bool {
        let should_block = should_i_block_call();
        let decision = if should_block {
            "WILL"
        } else {
            "WILL NOT"
        };
        println!("  >> [mdl-block] i {decision} block call to {}#{}...",
                 call.interface_name, call.function_name);

        should_block
    }
}

fn should_i_block_call() -> bool {
    // Returns the string value or an error if not set
    let var_value = env::var("SHOULD_BLOCK").unwrap_or_else(|_| "false".to_string());

    // Parse the string into a boolean
    let is_true: bool = var_value.parse().unwrap_or(false);
    is_true
}

// Export the component
bindings::export!( Service with_types_in bindings );
