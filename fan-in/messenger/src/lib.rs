mod bindings {
    wit_bindgen::generate!({
        world: "messenger-svc",
        generate_all
    });
}
use crate::bindings::exports::my::service::messenger::Guest;

pub struct Service;

impl Guest for Service {
    fn get_msg() -> String{
        "You made me swallow my gum!".to_string()
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
