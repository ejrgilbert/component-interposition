mod bindings {
    wit_bindgen::generate!({
        world: "shapes-handles-svc",
        async: true,
        generate_all
    });
}

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::bindings::exports::my::service::shapes_handles::{Counter, CounterBorrow, Guest};
use crate::bindings::exports::my::service::shapes_handles_types::Guest as TypesGuest;
use crate::bindings::exports::my::service::shapes_handles_types::GuestCounter;
use crate::bindings::exports::my::service::async_bucket::{
    Bucket as AsyncBucket, Guest as AsyncBucketGuest, GuestBucket as AsyncGuestBucket,
};
use wit_bindgen::{FutureReader, StreamReader};

pub struct Service;

pub struct CounterImpl {
    value: Cell<i32>,
}

impl GuestCounter for CounterImpl {
    async fn new(start: i32) -> Self {
        CounterImpl { value: Cell::new(start) }
    }
    async fn increment(&self) {
        self.value.set(self.value.get() + 1);
    }
    async fn current(&self) -> i32 {
        self.value.get()
    }
}

impl TypesGuest for Service {
    type Counter = CounterImpl;
}

impl Guest for Service {
    async fn make_counter(start: i32) -> Counter {
        Counter::new(CounterImpl::new(start).await)
    }

    async fn counter_current(c: CounterBorrow<'_>) -> i32 {
        c.get::<CounterImpl>().current().await
    }

    async fn consume_counter(c: Counter) -> i32 {
        c.into_inner::<CounterImpl>().current().await
    }

    async fn delayed_add(a: i32, b: i32) -> FutureReader<i32> {
        let (tx, rx) = bindings::wit_future::new(|| 0);
        wit_bindgen::spawn(async move {
            let _ = tx.write(a + b).await;
        });
        rx
    }

    async fn countdown(start: i32) -> StreamReader<i32> {
        let (mut tx, rx) = bindings::wit_stream::new();
        wit_bindgen::spawn(async move {
            for i in (0..=start).rev() {
                let _ = tx.write(vec![i]).await;
            }
        });
        rx
    }
}

// async-bucket: in-memory u32->u32 resource.
pub struct BucketImpl {
    store: RefCell<HashMap<u32, u32>>,
}

impl AsyncGuestBucket for BucketImpl {
    async fn new(_seed: u32) -> Self {
        BucketImpl { store: RefCell::new(HashMap::new()) }
    }
    async fn get(&self, key: u32) -> Option<u32> {
        self.store.borrow().get(&key).copied()
    }
    async fn put(&self, key: u32, val: u32) {
        self.store.borrow_mut().insert(key, val);
    }
}

impl AsyncBucketGuest for Service {
    type Bucket = BucketImpl;
    async fn open(seed: u32) -> AsyncBucket {
        AsyncBucket::new(BucketImpl::new(seed).await)
    }
}

bindings::export!( Service with_types_in bindings );
