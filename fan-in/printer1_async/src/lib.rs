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
    async fn print1_async() {
        println!("     [print1-async] entered!");
        println!("         it's dangerous to go alone! take this 🗡️");
        println!("     [print1-async] exit!");
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
