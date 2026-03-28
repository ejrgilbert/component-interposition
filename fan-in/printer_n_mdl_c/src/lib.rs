mod bindings {
    wit_bindgen::generate!({
        world: "printer-n-mdl",
        async: true,
        generate_all
    });
}
use crate::bindings::exports::my::service::printer_n::Guest;
use crate::bindings::my::service::printer_n::print_n;

pub struct Service;

impl Guest for Service {
    async fn print_n(
        msg: String,
        n: u32
    ) {

        println!("  >> [printer-n-mdlC] entered!");

        // Forward the request to the downstream handler
        // NOTE: This can be either the core service OR another middleware!
        print_n(msg, n).await;

        println!("  >> [printer-n-mdlC] exit!");
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
