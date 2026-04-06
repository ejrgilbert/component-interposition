mod bindings {
    wit_bindgen::generate!({
        world: "printer1-async-svc",
        async: true,
        generate_all
    });
}
use crate::bindings::exports::my::service::printer1_async::Guest;

pub struct Service;

impl Guest for Service {
    async fn print1() {
        println!("     [print1] entered!");
        println!("         it's dangerous to go alone! take this 🗡️");
        println!("     [print1] exit!");
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
