mod bindings {
    wit_bindgen::generate!({
        world: "printer-mdl",
        generate_all
    });
}
use crate::bindings::exports::my::service::type_erased_middleware::Guest;

pub struct Service;

impl Guest for Service {
    fn before_call(name: String) {
        println!("  >> [mdl-{name}] before!");
    }
    fn should_block_call(_name: String) -> bool {
        false
    }
    fn after_call(name: String) -> () {
        println!("  >> [mdl-{name}] after!");
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
