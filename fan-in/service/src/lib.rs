mod bindings {
    wit_bindgen::generate!({
        world: "service",
        async: true,
        generate_all
    });
}

use bindings::my::service::adder::add;
use bindings::my::service::adder_async::add_async;
use bindings::my::service::printer1::print1;
use bindings::my::service::printer_n::print_n;

use bindings::exports::wasi::http::handler::Guest;
use bindings::exports::wasi::http::handler;
use bindings::wasi::http::types::{Response, Request};
use bindings::wit_future;

pub struct Service;

impl Guest for Service {
    async fn handle(
        request: handler::Request,
    ) -> Result<handler::Response, handler::ErrorCode> {

        println!("[svc] entered!");

        let (a, b) = (1, 2);
        let result = add(a, b).await;
        
        println!("[svc] adder says '{a} + {b} = {result}'");

        let (a, b) = (1, 2);
        let result_async = add_async(a, b).await;

        println!("[svc] adder-async says '{a} + {b} = {result_async}'");

        let str = "it's dangerous to go alone! take this 🗡️";
        print1();
        println!("[svc] printer1 completed!");

        print_n(str.to_string(), 4).await;
        println!("[svc] printer-n completed!");

        println!("[svc] exit!");

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
