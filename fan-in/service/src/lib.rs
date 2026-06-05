mod bindings {
    wit_bindgen::generate!({
        world: "service",
        async: true,
        generate_all
    });
}

use bindings::my::service::adder::add;
use bindings::my::service::adder_async::add_async;
use bindings::my::service::messenger::get_msg;
use bindings::my::service::messenger_async::get_msg_async;
use bindings::my::service::printer1_async::print1_async;
use bindings::my::service::printer1::print1;
use bindings::my::service::printer_n::print_n;
use bindings::my::service::shapes::{
    self, AccessPerms, Event, Person, Priority,
};
use bindings::my::service::shapes_handles::{self, Counter};
use bindings::my::service::async_bucket::{self as async_bucket, Bucket as AsyncBucket};

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

        let msg = get_msg().await;
        println!("[svc] get-msg:       '{msg}'");
        let msg_async = get_msg_async().await;
        println!("[svc] get-msg-async: '{msg_async}'");

        let str = "it's dangerous to go alone! take this 🗡️";
        print1(str.to_string()).await;
        println!("[svc] printer1 completed!");
        print1_async(str.to_string()).await;
        println!("[svc] printer1-async completed!");

        print_n(str.to_string(), 4).await;
        println!("[svc] printer-n completed!");

        // shapes interface — exercise every shape so its imports are linked
        let _ = shapes::pick_color(Priority::High).await;
        let _ = shapes::check_perms(AccessPerms::READ | AccessPerms::WRITE).await;
        let _ = shapes::greet(Person { name: "Link".to_string(), age: 17 }).await;
        let _ = shapes::describe_event(Event::Click(42)).await;
        let _ = shapes::swap_pair((7, "hello".to_string())).await;
        let _ = shapes::maybe_double(Some(21)).await;
        let _ = shapes::divide(10, 3).await;
        let _ = shapes::sum(vec![1, 2, 3, 4]).await;
        let _ = shapes::to_string('Z').await;
        let _ = shapes::aggregate(
            Person { name: "Zelda".to_string(), age: 18 },
            Some(2),
        ).await;

        // shapes-handles interface — exercise resource constructor + methods,
        // make-counter / counter-current / consume-counter, and the future
        // and stream return shapes so all imports are linked.
        let direct = Counter::new(5).await;
        direct.increment().await;
        let _ = direct.current().await;
        drop(direct);

        let counter: Counter = shapes_handles::make_counter(10).await;
        counter.increment().await;
        let _ = shapes_handles::counter_current(&counter).await;
        let _ = shapes_handles::consume_counter(counter).await;

        let fut = shapes_handles::delayed_add(40, 2).await;
        let _ = fut.await;

        let stream = shapes_handles::countdown(3).await;
        let _ = stream.collect().await;

        // Resource with all async funcs
        let b1: AsyncBucket = AsyncBucket::new(1).await;
        b1.put(10, 100).await;
        let _ = b1.get(10).await;
        drop(b1);

        let b2: AsyncBucket = async_bucket::open(2).await;
        b2.put(20, 200).await;
        let _ = b2.get(20).await;
        drop(b2);

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
