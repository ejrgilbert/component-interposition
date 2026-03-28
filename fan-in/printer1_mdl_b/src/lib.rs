mod bindings {
    wit_bindgen::generate!({
        world: "printer1-mdl",
        async: true,
        generate_all
    });
}
use crate::bindings::exports::my::service::printer1::Guest;
use crate::bindings::my::service::printer1::print1;

pub struct Service;

impl Guest for Service {
    async fn print1(
        msg: String
    ) {

        println!("  >> [printer1-mdlB] entered!");

        // Forward the request to the downstream handler
        // NOTE: This can be either the core service OR another middleware!
        print1(msg).await;

        println!("  >> [printer1-mdlB] exit!");
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
