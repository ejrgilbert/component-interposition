mod bindings {
    wit_bindgen::generate!({
        world: "printer-mdl",
        generate_all
    });
}

use crate::bindings::exports::splicer::tier1::before::Guest as BeforeGuest;
use crate::bindings::exports::splicer::tier1::after::Guest as AfterGuest;
use crate::bindings::splicer::common::types::CallId;

pub struct Service;

impl BeforeGuest for Service {
    fn on_call(call: CallId) {
        println!("  >> [mdl-{}#{}] before!", call.interface_name, call.function_name);
    }
}

impl AfterGuest for Service {
    fn on_return(call: CallId) -> () {
        println!("  >> [mdl-{}#{}] after!", call.interface_name, call.function_name);
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
