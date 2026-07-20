mod bindings {
    wit_bindgen::generate!({
        world: "subgraph-service-svc",
        async: true,
        generate_all
    });
}

use bindings::exports::wasi::http::handler::{self as handler, Guest};
use bindings::my::service::shapes_handles::{self, Counter};
use bindings::my::service::shapes_viewer;
use bindings::wasi::http::types::{Request, Response};
use bindings::wit_future;

pub struct SubgraphService;

impl Guest for SubgraphService {
    async fn handle(
        request: handler::Request,
    ) -> Result<handler::Response, handler::ErrorCode> {
        // shapes-viewer calls: view() does not cross the subgraph boundary.
        // view-with-counter() passes a T' handle through the collateral interface;
        // the edge shim unwraps it before forwarding to shapes-viewer-comp.
        let viewer_result = shapes_viewer::view().await;
        println!("[subgraph-svc] shapes-viewer says: {viewer_result}");

        let counter_for_viewer: Counter = shapes_handles::make_counter(7).await;
        let viewer_result2 = shapes_viewer::view_with_counter(counter_for_viewer).await;
        println!("[subgraph-svc] view-with-counter says: {viewer_result2}");

        // These calls cross the subgraph boundary and are instrumented via T'.
        let direct = Counter::new(5).await;
        direct.increment().await;
        let _ = direct.current().await;
        drop(direct);

        let counter: Counter = shapes_handles::make_counter(10).await;
        counter.increment().await;
        let _ = shapes_handles::counter_current(&counter).await;
        let _ = shapes_handles::consume_counter(counter).await;

        // Exercise delayed-add and countdown so their imports appear in the component type.
        let fut = shapes_handles::delayed_add(40, 2).await;
        let _ = fut.await;

        let stream = shapes_handles::countdown(3).await;
        let _ = stream.collect().await;

        println!("[subgraph-svc] done");

        let headers = request.get_headers().await;
        let (_, result_rx) = wit_future::new(|| Ok(()));
        let (body, trailers) = Request::consume_body(request, result_rx).await;
        Ok(Response::new(headers, Some(body), trailers).await.0)
    }
}

bindings::export!(SubgraphService with_types_in bindings);
