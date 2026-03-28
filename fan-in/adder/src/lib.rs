mod bindings {
    wit_bindgen::generate!({
        world: "adder-svc",
        async: true,
        generate_all
    });
}
use crate::bindings::exports::my::service::adder::Guest;

pub struct Service;

impl Guest for Service {
    async fn add(
        a: i32, b: i32
    ) -> i32 {

        println!("     [adder] entered!");

        let res = a + b;

        println!("     [adder] exit!");

        res
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
