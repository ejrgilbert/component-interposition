mod bindings {
    wit_bindgen::generate!({
        world: "printer-mdl",
        async: true,
        generate_all
    });
}

use crate::bindings::exports::splicer::proxy::before::Guest as BeforeGuest;
use crate::bindings::exports::splicer::proxy::after::Guest as AfterGuest;

pub struct Service;

impl BeforeGuest for Service {
    async fn before_call(name: String) {
        println!("  >> [mdl-{name}] before!");
    }
}

impl AfterGuest for Service {
    async fn after_call(name: String) -> () {
        println!("  >> [mdl-{name}] after!");
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
