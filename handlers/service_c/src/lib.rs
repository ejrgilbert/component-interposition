mod bindings {
    wit_bindgen::generate!({
        world: "service",
        async: true,
        generate_all
    });
}

use bindings::exports::wasi::http::handler::Guest;
use bindings::exports::wasi::http::handler;

use crate::bindings::wasi::http::handler::handle;

pub struct Service;

impl Guest for Service {
    async fn handle(
        request: handler::Request,
    ) -> Result<handler::Response, handler::ErrorCode> {

        println!("    [svcC] entered!");

        // Nothing fancy, just send the request to the downstream service.
        let response = handle(request).await?;
        
        println!("    [svcC] received response from svcA!");

        Ok(response)
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
