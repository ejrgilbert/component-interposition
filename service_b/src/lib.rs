mod bindings {
    wit_bindgen::generate!({
        world: "service",
        async: true,
        generate_all
    });
}

use bindings::exports::wasi::http::handler::Guest;
use bindings::exports::wasi::http::handler;
use bindings::wasi::http::types::{Response, Request};
use bindings::wit_future;

pub struct Service;

impl Guest for Service {
    async fn handle(
        request: handler::Request,
    ) -> Result<handler::Response, handler::ErrorCode> {
        println!("    [svcB] entered!");
        
        // Just copy the request's headers
        let headers = request.get_headers().await;

        // Just copy the request's body
        let (_, result_rx) = wit_future::new(|| Ok(()));
        let (body, trailers) = Request::consume_body(request, result_rx).await;

        Ok(Response::new(headers, Some(body), trailers).await.0)
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
