mod bindings {
    wit_bindgen::generate!({
        world: "middleware",
        async: true,
        generate_all
    });
}

use bindings::exports::wasi::http::handler::Guest;
use bindings::wasi::http::handler;

/// Logging HTTP middleware
struct LoggingMiddleware;

impl Guest for LoggingMiddleware {
    async fn handle(
        request: handler::Request,
    ) -> Result<handler::Response, handler::ErrorCode> {
        log(">>> [mdlB] enter");

        // Forward the request to the downstream handler
        // NOTE: This can be either the core service OR another middleware!
        let response = handler::handle(request).await?;

        log("<<< [mdlB] exit");

        Ok(response)
    }
}

/// Helper to write to stdout

fn log(msg: &str) {
    println!("{msg}")
}


// Export the component
bindings::export!( LoggingMiddleware with_types_in bindings );
