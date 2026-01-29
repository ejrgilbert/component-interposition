mod bindings {
    wit_bindgen::generate!({
        world: "middleware",
//         inline: "
// package my:logging-middleware;
//
// world middleware {
//   include wasi:http/middleware@0.3.0-rc-2026-01-06;
// }",
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
        log(">>> logging middleware reached\n");

        // Forward the request to the downstream handler
        let response = handler::handle(request).await?;

        log("<<< logging middleware returning response\n");

        Ok(response)
    }
}

/// Helper to write to stdout

fn log(msg: &str) {
    println!("{msg}")
}


// Export the component
bindings::export!( LoggingMiddleware with_types_in bindings );
