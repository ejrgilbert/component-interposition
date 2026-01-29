mod bindings {
    wit_bindgen::generate!({
        world: "service",
        async: true,
        generate_all
    });
}

use crate::bindings::exports::my::service::handler::Guest;
use crate::bindings::exports::my::service::handler;

pub struct Service;

impl Guest for Service {
    async fn handle(
        request: handler::Request,
    ) -> Result<String, handler::ErrorCode> {
        // Simple echo response
        let body = format!("Service received: {}", request);
        
        // Print to stdout for debugging
        let msg = format!("Logging in service: {}\n", request);
        println!("{msg}");
        
        Ok(body)
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
