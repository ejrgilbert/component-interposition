mod bindings {
    wit_bindgen::generate!({
        world: "printer1-svc",
        generate_all
    });
}
use crate::bindings::exports::my::service::printer1::Guest;

pub struct Service;

impl Guest for Service {
    fn print1(msg: String) {
        println!("     [print1] entered!");
        println!("         {msg}");
        println!("     [print1] exit!");
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
