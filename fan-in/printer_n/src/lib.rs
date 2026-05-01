mod bindings {
    wit_bindgen::generate!({
        world: "printer-n-svc",
        generate_all
    });
}
use crate::bindings::exports::my::service::printer_n::Guest;

pub struct Service;

impl Guest for Service {
    fn print_n(
        msg: String,
        n: u32
    ) {
        println!("     [printN] entered!");
        for _ in 0..n {
            println!("         {msg}");
        }
        println!("     [printN] exit!");
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
