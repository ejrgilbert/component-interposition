mod bindings {
    wit_bindgen::generate!({
        world: "blocker-mdl",
        async: true,
        generate_all
    });
}

use crate::bindings::exports::splicer::proxy::blocking::Guest;

pub struct Service;

impl Guest for Service {
    async fn should_block_call(name: String) -> bool {
        println!("[mdl-block] blocking call to {name}...");
        true
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
