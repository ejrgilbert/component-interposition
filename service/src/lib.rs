mod bindings {
    wit_bindgen::generate!({
        world: "service",
        async: true,
        generate_all
    });
}

use std::io::Write;
use crate::bindings::exports::wasi::http::handler::Guest;
use crate::bindings::exports::wasi::http::handler;
use crate::bindings::wasi::http::types::{Headers, Response, Request};
use flate2::{
    Compression,
    write::{DeflateDecoder, DeflateEncoder},
};
use wit_bindgen::StreamResult;
use crate::bindings::{wit_future, wit_stream};

pub struct Service;

impl Guest for Service {
    async fn handle(
        request: handler::Request,
    ) -> Result<handler::Response, handler::ErrorCode> {
        let headers = request.get_headers().await;
        let (_, result_rx) = wit_future::new(|| Ok(()));
        let (body, trailers) = Request::consume_body(request, result_rx).await;

        let (response, _result) = if headers
            .get("x-host-to-host".parse().unwrap()).await
            .into_iter()
            .any(|v| v == b"true")
        {
            // This is the easy and efficient way to do it...
            Response::new(headers, Some(body), trailers).await
        } else {
            // ...but we do it the more difficult, less efficient way here to exercise various component model
            // features (e.g. `future`s, `stream`s, and post-return asynchronous execution):
            let (trailers_tx, trailers_rx) = wit_future::new(|| todo!());
            let (mut pipe_tx, pipe_rx) = wit_stream::new();

            wit_bindgen::spawn(async move {
                let mut body_rx = body;
                let mut chunk = Vec::with_capacity(1024);
                loop {
                    let (status, buf) = body_rx.read(chunk).await;
                    chunk = buf;
                    match status {
                        StreamResult::Complete(_) => {
                            chunk = pipe_tx.write_all(chunk).await;
                            assert!(chunk.is_empty());
                        }
                        StreamResult::Dropped => break,
                        StreamResult::Cancelled => unreachable!(),
                    }
                }

                drop(pipe_tx);

                trailers_tx.write(trailers.await).await.unwrap();
            });

            Response::new(headers, Some(pipe_rx), trailers_rx).await
        };

        println!("[SERVICE] hello world!");

        Ok(response)


        // Simple echo response
        // let body = format!("Service received: {}", request);
        //
        // // Print to stdout for debugging
        // let msg = format!("Logging in service: {}\n", request);
        // println!("{msg}");
        //
        // Ok(body)
    }
}

// Export the component
bindings::export!( Service with_types_in bindings );
