use std::env;
use std::future::Future;
use bytes::Bytes;
use flate2::Compression;
use flate2::write::{DeflateDecoder, DeflateEncoder};
use futures::SinkExt;
use http::HeaderValue;
use http_body::Body;
use http_body_util::{BodyExt as _, Collected, combinators::UnsyncBoxBody};
use std::io::Write;
use tokio::try_join;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Result, Store, WasmBacktraceDetails};
use wasmtime_wasi::{TrappableError, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::p3::bindings::http::types::ErrorCode;
use wasmtime_wasi_http::p3::{
    self, Request, RequestOptions, WasiHttpCtx, WasiHttpCtxView, WasiHttpView,
};
use wasmtime_wasi_http::types::DEFAULT_FORBIDDEN_HEADERS;

#[tokio::main]
async fn main() -> Result<()> {
    let main = env::args().nth(1).context("usage: runner <main.wasm>")?;

    println!("\nRunning the echo test with host-to-host disabled");
    test_http_echo(&main, false, false).await?;
    // println!("\nRunning the echo test with host-to-host enabled");
    // test_http_echo(&main, true, true).await?;

    Ok(())
}

async fn test_http_echo(component: &str, use_compression: bool, host_to_host: bool) -> Result<()> {
    _ = env_logger::try_init();

    let body = b"And the mome raths outgrabe";

    // Prepare the raw body, optionally compressed if that's what we're testing.
    let raw_body = if use_compression {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(body)?;
        Bytes::from(encoder.finish()?)
    } else {
        Bytes::copy_from_slice(body)
    };

    // Prepare the http_body body, modeled here as a channel with the body
    // chunk above buffered up followed by some trailers. Note that trailers
    // are always here to test that code paths throughout the components.
    let (mut body_tx, body_rx) = futures::channel::mpsc::channel::<Result<_, ErrorCode>>(1);

    // Build the `http::Request`, optionally specifying compression-related headers.
    let mut request = http::Request::builder()
        .uri("http://localhost/")
        .method(http::Method::GET)
        .header("foo", "bar");
    if use_compression {
        request = request
            .header("content-encoding", "deflate")
            .header("accept-encoding", "nonexistent-encoding, deflate");
    }
    if host_to_host {
        request = request.header("x-host-to-host", "true");
    }

    // Create the HTTP request using the receiver
    let request = request.body(http_body_util::StreamBody::new(body_rx))?;

    // Spawn an async task to feed the body
    let send_body_task = async move {
        let _ = body_tx
            .send(Ok(http_body::Frame::data(raw_body)))
            .await;

        let _ = body_tx
            .send(Ok(http_body::Frame::trailers({
                let mut trailers = http::HeaderMap::new();
                assert!(
                    trailers
                        .insert("fizz", HeaderValue::from_static("buzz"))
                        .is_none()
                );
                trailers
            })))
            .await;
        drop(body_tx);
    };

    // Send this request to wasm and assert that success comes back.
    //
    // Note that this will read the entire body internally and wait for
    // everything to get collected before proceeding to below.
    let (store_tx, _) = tokio::sync::mpsc::channel::<UnsyncBoxBody<Bytes, ErrorCode>>(8);
    let response = futures::join!(
        run_http(
            component,
            request,
            store_tx
        ),
        send_body_task
    )
        .0?     // result from run_http
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);

    // Our input header should be echo'd back.
    assert_eq!(
        response.headers().get("foo"),
        Some(&HeaderValue::from_static("bar"))
    );

    // The compression headers should be set if `use_compression` was turned on.
    if use_compression {
        assert_eq!(
            response.headers().get("content-encoding"),
            Some(&HeaderValue::from_static("deflate"))
        );
        assert!(response.headers().get("content-length").is_none());
    }

    // Trailers should be echo'd back as well.
    let trailers = response.body().trailers().expect("trailers missing");
    assert_eq!(
        trailers.get("fizz"),
        Some(&HeaderValue::from_static("buzz"))
    );

    // And our body should match our original input body as well.
    let (_, collected_body) = response.into_parts();
    let collected_body = collected_body.to_bytes();

    let response_body = if use_compression {
        let mut decoder = DeflateDecoder::new(Vec::new());
        decoder.write_all(&collected_body)?;
        decoder.finish()?
    } else {
        collected_body.to_vec()
    };
    assert_eq!(response_body, body.as_slice());
    Ok(())
}

async fn run_http<E: Into<ErrorCode> + 'static>(
    component_filename: &str,
    req: http::Request<impl Body<Data = Bytes, Error = E> + Send + Sync + 'static>,
    request_body_tx: Sender<UnsyncBoxBody<Bytes, ErrorCode>>,
) -> Result<Result<http::Response<Collected<Bytes>>, Option<ErrorCode>>> {

    let engine = engine(|config| {
        config.async_support(true);
        config.wasm_component_model_async(true);
        config.wasm_component_model_async_stackful(true);
        config.wasm_backtrace_details(WasmBacktraceDetails::Enable);
    });
    let component = Component::from_file(&engine, component_filename)?;

    let mut store = Store::new(&engine, Ctx::new(request_body_tx.clone()));

    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)
        .context("failed to link `wasi:cli@0.2.x`")?;
    wasmtime_wasi::p3::add_to_linker(&mut linker).context("failed to link `wasi:cli@0.3.x`")?;
    wasmtime_wasi_http::p3::add_to_linker(&mut linker)
        .context("failed to link `wasi:http@0.3.x`")?;
    let service = wasmtime_wasi_http::p3::bindings::Service::instantiate_async(&mut store, &component, &linker).await?;

    let (req, io) = Request::from_http(req);
    let (tx, rx) = tokio::sync::oneshot::channel();
    let ((handle_result, ()), res) = try_join!(
        async move {
            store
                .run_concurrent(async |store| {
                    try_join!(
                        async {
                            let (res, task) = match service.handle(store, req).await? {
                                Ok(pair) => pair,
                                Err(err) => return Ok(Err(Some(err))),
                            };
                            _ = tx
                                .send(store.with(|store| res.into_http(store, async { Ok(()) }))?);
                            task.block(store).await;
                            Ok(Ok(()))
                        },
                        async { io.await.context("failed to consume request body") }
                    )
                })
                .await?
        },
        async move {
            let res = rx.await?;
            let (parts, body) = res.into_parts();
            let body = body.collect().await.context("failed to collect body")?;
            Ok(http::Response::from_parts(parts, body))
        }
    )?;

    Ok(handle_result.map(|()| res))
}

struct TestHttpCtx {
    request_body_tx: Option<Sender<UnsyncBoxBody<Bytes, ErrorCode>>>,
}

impl WasiHttpCtx for TestHttpCtx {
    fn is_forbidden_header(&mut self, name: &http::header::HeaderName) -> bool {
        name.as_str() == "custom-forbidden-header" || DEFAULT_FORBIDDEN_HEADERS.contains(name)
    }

    fn send_request(
        &mut self,
        request: http::Request<UnsyncBoxBody<Bytes, ErrorCode>>,
        options: Option<RequestOptions>,
        fut: Box<dyn Future<Output = Result<(), ErrorCode>> + Send>,
    ) -> Box<
        dyn Future<
            Output = Result<
                (
                    http::Response<UnsyncBoxBody<Bytes, ErrorCode>>,
                    Box<dyn Future<Output = Result<(), ErrorCode>> + Send>,
                ),
                TrappableError<ErrorCode>,
            >,
        > + Send,
    > {
        println!("Sending request inside ctx");
        _ = fut;
        if let Some("p3-test") = request.uri().authority().map(|v| v.as_str()) {
            _ = self
                .request_body_tx
                .take()
                .unwrap()
                .send(request.into_body());
            Box::new(async {
                Ok((
                    http::Response::new(Default::default()),
                    Box::new(async { Ok(()) }) as Box<dyn Future<Output = _> + Send>,
                ))
            })
        } else {
            Box::new(async move {
                use http_body_util::BodyExt;

                let (res, io) = p3::default_send_request(request, options).await?;
                Ok((
                    res.map(BodyExt::boxed_unsync),
                    Box::new(io) as Box<dyn Future<Output = _> + Send>,
                ))
            })
        }
    }
}

struct Ctx {
    table: ResourceTable,
    wasi: WasiCtx,
    http: TestHttpCtx,
}

impl Ctx {
    fn new(request_body_tx: Sender<UnsyncBoxBody<Bytes, ErrorCode>>) -> Self {
        Self {
            table: ResourceTable::default(),
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            http: TestHttpCtx {
                request_body_tx: Some(request_body_tx),
            },
        }
    }
}

impl WasiView for Ctx {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl WasiHttpView for Ctx {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
        }
    }
}

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use anyhow::Context;
use tokio::sync::mpsc::Sender;
use wasmtime::{CacheStore, Config, Engine};

/// Helper to create an `Engine` with a pre-configured `Config` that uses a
/// cache for faster building of modules.
pub fn engine(configure: impl FnOnce(&mut Config)) -> Engine {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config
        .enable_incremental_compilation(cache_store())
        .unwrap();
    configure(&mut config);
    Engine::new(&config).unwrap()
}

// Simple incremental cache used during tests to help improve test runtime.
//
// Many tests take a similar module (e.g. a component, a preview1 thing, sync,
// async, etc) and run it in different contexts and this improve cache hit rates
// across usages by sharing one incremental cache across tests.
fn cache_store() -> Arc<dyn CacheStore> {
    #[derive(Debug)]
    struct MyCache;

    static CACHE: Mutex<Option<HashMap<Vec<u8>, Vec<u8>>>> = Mutex::new(None);

    impl CacheStore for MyCache {
        fn get(&self, key: &[u8]) -> Option<Cow<'_, [u8]>> {
            let mut cache = CACHE.lock().unwrap();
            let cache = cache.get_or_insert_with(HashMap::new);
            cache.get(key).map(|s| s.to_vec().into())
        }

        fn insert(&self, key: &[u8], value: Vec<u8>) -> bool {
            let mut cache = CACHE.lock().unwrap();
            let cache = cache.get_or_insert_with(HashMap::new);
            cache.insert(key.to_vec(), value);
            true
        }
    }

    Arc::new(MyCache)
}
