mod bindings {
    wit_bindgen::generate!({
        world: "shapes-viewer-svc",
        async: true,
        generate_all
    });
}

use bindings::exports::my::service::shapes_viewer::Guest;
use bindings::my::service::shapes_handles::{self, Counter};

pub struct ShapesViewer;

// Shared impl so view() can call view-with-counter without trait dispatch.
async fn do_view_with_counter(c: Counter) -> i32 {
    c.increment().await;
    c.current().await
}

impl Guest for ShapesViewer {
    // All calls here go directly to shapes-handles-comp — no T' wrapper,
    // no instrumentation. Demonstrates the contrast with subgraph-service's
    // instrumented calls.
    async fn view() -> i32 {
        let direct = Counter::new(5).await;
        direct.increment().await;
        let v1 = direct.current().await; // 6

        // Pass the counter to view-with-counter: handle moves within the
        // external component (outside the subgraph), no boundary crossing.
        let counter = shapes_handles::make_counter(10).await;
        let v2 = do_view_with_counter(counter).await; // 11 (10 + 1 increment)

        // Exercise remaining freestanding shapes-handles functions so they
        // appear in the component type and match shapes-handles-comp's export.
        let borrow_c = Counter::new(0).await;
        let _ = shapes_handles::counter_current(&borrow_c).await;
        let own_c = Counter::new(1).await;
        let _ = shapes_handles::consume_counter(own_c).await;

        let fut = shapes_handles::delayed_add(1, 2).await;
        let _ = fut.await;

        let stream = shapes_handles::countdown(1).await;
        let _ = stream.collect().await;

        v1 + v2 // 17
    }

    // Receives an owned counter handle from a caller. Increments and returns
    // current value. When called via the T' wrapper (from inside the subgraph),
    // the T' wrapper unwraps T' -> raw before this function sees the counter.
    async fn view_with_counter(c: Counter) -> i32 {
        do_view_with_counter(c).await
    }
}

bindings::export!(ShapesViewer with_types_in bindings);
