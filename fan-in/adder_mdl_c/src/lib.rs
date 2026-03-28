mod bindings {
    wit_bindgen::generate!({
        world: "adder-mdl",
        async: true,
        generate_all
    });
}
use crate::bindings::exports::my::service::adder::Guest;
use crate::bindings::my::service::adder::add;

pub struct Service;

impl Guest for Service {
    async fn add(
        a: i32, b: i32
    ) -> i32 {

        println!("  >> [adder-mdlC] entered!");

        // Forward the request to the downstream handler
        // NOTE: This can be either the core service OR another middleware!
        let res = add(a, b).await;

        println!("  >> [adder-mdlC] exit!");

        res
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
