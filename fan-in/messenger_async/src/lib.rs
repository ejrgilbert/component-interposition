mod bindings {
    wit_bindgen::generate!({
        world: "messenger-async-svc",
        async: true,
        generate_all
    });
}
use crate::bindings::exports::my::service::messenger_async::Guest;

pub struct Service;

impl Guest for Service {
    async fn get_msg_async() -> String{
        "That's going to be in my digestive tract for seven years!".to_string()
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
