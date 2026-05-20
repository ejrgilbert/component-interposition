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
use wasmtime::component::{Component, Linker, ResourceTable, Val};
use wasmtime::{Result, Store, WasmBacktraceDetails};
use wasmtime_wasi::{
    DirPerms, FilePerms, TrappableError, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView,
};
use wasmtime_wasi_http::p3::bindings::http::types::ErrorCode;
use wasmtime_wasi_http::p3::{
    self, Request, RequestOptions, WasiHttpCtx, WasiHttpCtxView, WasiHttpView,
};
use wasmtime_wasi_http::types::DEFAULT_FORBIDDEN_HEADERS;

#[tokio::main]
async fn main() -> Result<()> {
    let main_arg = env::args().nth(1).context("usage: runner <main.wasm>")?;
    let loop_n = loop_n_from_env();
    _ = env_logger::try_init();

    // Build the wasmtime engine + linker once, then instantiate the
    // spliced component into a single long-lived `Store` that all
    // `LOOP_N` iterations share. This matches how a real HTTP server
    // hosts a wasm component (one instance, many requests) and lets
    // stateful builtins like `otel-bare-metrics` actually accumulate
    // samples across requests rather than seeing a fresh accumulator
    // every call.
    let engine = engine(|config| {
        config.async_support(true);
        config.wasm_component_model_async(true);
        config.wasm_component_model_async_stackful(true);
        config.wasm_backtrace_details(WasmBacktraceDetails::Enable);
    });
    let component = Component::from_file(&engine, &main_arg)?;

    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)
        .context("failed to link `wasi:cli@0.2.x`")?;
    wasmtime_wasi::p3::add_to_linker(&mut linker).context("failed to link `wasi:cli@0.3.x`")?;
    wasmtime_wasi_http::p3::add_to_linker(&mut linker)
        .context("failed to link `wasi:http@0.3.x`")?;
    wire_otel_stub(&mut linker).context("failed to link `wasi:otel/*` stubs")?;

    // `request_body_tx` on the host context is only consumed when the
    // wasm makes an outgoing request against the `p3-test` authority
    // (a host-to-host plumbing test). The echo path doesn't hit it,
    // so we wire a dummy channel that lives for the run.
    let (dummy_tx, _dummy_rx) = tokio::sync::mpsc::channel::<UnsyncBoxBody<Bytes, ErrorCode>>(1);
    let mut store = Store::new(&engine, Ctx::new(dummy_tx));
    let service = wasmtime_wasi_http::p3::bindings::Service::instantiate_async(
        &mut store, &component, &linker,
    )
    .await?;

    println!("\nRunning the echo test with host-to-host disabled");
    for i in 1..=loop_n {
        if loop_n > 1 {
            println!("--- iteration {i}/{loop_n} ---");
        }
        test_http_echo(&mut store, &service, false, false).await?;
    }

    Ok(())
}

/// Number of `test_http_echo` iterations per run. Driven by `LOOP_N`
/// (default `1`). Used by the `--otel` demo path in `run.sh` to
/// generate enough wrapped calls that the `otel-bare-metrics`
/// delta-window actually closes and emits a `[OTEL/METRIC]` line.
fn loop_n_from_env() -> u32 {
    env::var("LOOP_N")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(1)
}

async fn test_http_echo(
    store: &mut Store<Ctx>,
    service: &wasmtime_wasi_http::p3::bindings::Service,
    use_compression: bool,
    host_to_host: bool,
) -> Result<()> {
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
    let response = futures::join!(do_one_request(store, service, request), send_body_task)
        .0?     // result from do_one_request
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

/// Dispatch a single HTTP request against the long-lived spliced
/// wasm instance owned by `store` + `service`. Called once per
/// `LOOP_N` iteration so stateful builtins see the same instance.
async fn do_one_request<E: Into<ErrorCode> + 'static>(
    store: &mut Store<Ctx>,
    service: &wasmtime_wasi_http::p3::bindings::Service,
    req: http::Request<impl Body<Data = Bytes, Error = E> + Send + Sync + 'static>,
) -> Result<Result<http::Response<Collected<Bytes>>, Option<ErrorCode>>> {
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

const SHOULD_BLOCK: &str = "SHOULD_BLOCK";
/// Env var pointing at a host directory to preopen as the guest's
/// `.`. Set by demos that need filesystem access (e.g. `--builtin-
/// recorder`'s file sink). Unset = no preopen, identical to before.
const PREOPEN_DIR: &str = "PREOPEN_DIR";

struct Ctx {
    table: ResourceTable,
    wasi: WasiCtx,
    http: TestHttpCtx,
}

impl Ctx {
    fn new(request_body_tx: Sender<UnsyncBoxBody<Bytes, ErrorCode>>) -> Self {
        let mut builder = WasiCtxBuilder::new();
        builder
            .env(SHOULD_BLOCK, Self::get_should_block())
            .inherit_stdio();
        if let Ok(path) = env::var(PREOPEN_DIR) {
            builder
                .preopened_dir(&path, ".", DirPerms::all(), FilePerms::all())
                .unwrap_or_else(|e| panic!("preopen {path:?} failed: {e:#}"));
        }
        Self {
            table: ResourceTable::default(),
            wasi: builder.build(),
            http: TestHttpCtx {
                request_body_tx: Some(request_body_tx),
            },
        }
    }
    fn get_should_block() -> String {
        env::var(SHOULD_BLOCK)
            .unwrap_or_else(|_| "false".to_string())
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

// ── wasi:otel stub: print signals to stdout ─────────────────────────
//
// Tier-1 OTel builtins (`otel-bare-spans` / `otel-bare-metrics` /
// `otel-bare-logs`) import `wasi:otel/{tracing,metrics,logs}` against
// `0.2.0-rc.2`. The runner has no real OTLP exporter wired up; this
// stub satisfies the imports and prints a compact one-liner per
// signal so you can eyeball the flow during local demos. Replace
// with a proper OTLP bridge to ship into Grafana/Tempo/Prometheus.

const OTEL_METRICS: &str = "wasi:otel/metrics@0.2.0-rc.2";
const OTEL_TRACING: &str = "wasi:otel/tracing@0.2.0-rc.2";
const OTEL_LOGS: &str = "wasi:otel/logs@0.2.0-rc.2";

fn wire_otel_stub<T: 'static>(linker: &mut Linker<T>) -> Result<()> {
    let mut metrics = linker.instance(OTEL_METRICS)?;
    metrics.func_new("export", |_store, _ty, params, results| {
        print_metrics_export(&params[0]);
        // result<_, error> — ok side has no payload.
        results[0] = Val::Result(Ok(None));
        Ok(())
    })?;

    let mut tracing = linker.instance(OTEL_TRACING)?;
    tracing.func_new("on-start", |_store, _ty, params, _results| {
        print_span_start(&params[0]);
        Ok(())
    })?;
    tracing.func_new("on-end", |_store, _ty, params, _results| {
        print_span_end(&params[0]);
        Ok(())
    })?;
    tracing.func_new("outer-span-context", |_store, _ty, _params, results| {
        // No host-side parent — return an all-empty context so the
        // builtin mints a fresh trace-id.
        results[0] = empty_span_context();
        Ok(())
    })?;

    let mut logs = linker.instance(OTEL_LOGS)?;
    logs.func_new("on-emit", |_store, _ty, params, _results| {
        print_log_emit(&params[0]);
        Ok(())
    })?;

    Ok(())
}

fn print_metrics_export(v: &Val) {
    let rm = match as_record(v) {
        Some(r) => r,
        None => return,
    };
    let Some(Val::List(scope_metrics)) = field(rm, "scope-metrics") else {
        return;
    };
    for sm in scope_metrics {
        let Some(sm) = as_record(sm) else { continue };
        let Some(Val::List(metrics)) = field(sm, "metrics") else {
            continue;
        };
        for m in metrics {
            let Some(m) = as_record(m) else { continue };
            let name = field(m, "name").and_then(as_string).unwrap_or("?");
            let summary = field(m, "data")
                .and_then(summarize_metric_data)
                .unwrap_or_default();
            println!("[OTEL/METRIC] {name} {summary}");
        }
    }
}

/// Extract a one-line summary from a `metric-data` variant. Handles
/// the two cases the bare-metrics builtin emits (`u64-sum`,
/// `f64-histogram`); falls through to "?" for anything else.
fn summarize_metric_data(v: &Val) -> Option<String> {
    let Val::Variant(case, payload) = v else {
        return None;
    };
    let payload = payload.as_deref()?;
    let dp_list = match case.as_str() {
        "u64-sum" | "f64-sum" => field(as_record(payload)?, "data-points")?,
        "u64-histogram" | "f64-histogram" => field(as_record(payload)?, "data-points")?,
        _ => return Some(format!("(kind={case})")),
    };
    let Val::List(dps) = dp_list else { return None };
    let dp = as_record(dps.first()?)?;
    Some(match case.as_str() {
        "u64-sum" | "f64-sum" => format!("value={}", debug_val(field(dp, "value")?)),
        "u64-histogram" | "f64-histogram" => {
            let count = as_u64(field(dp, "count")?).unwrap_or(0);
            let sum = field(dp, "sum").map(debug_val).unwrap_or_default();
            format!("count={count} sum={sum}")
        }
        _ => String::new(),
    })
}

fn print_span_start(v: &Val) {
    let Some(ctx) = as_record(v) else { return };
    let trace = field(ctx, "trace-id").and_then(as_string).unwrap_or("?");
    let span = field(ctx, "span-id").and_then(as_string).unwrap_or("?");
    println!("[OTEL/SPAN-START] trace={trace} span={span}");
}

fn print_span_end(v: &Val) {
    let Some(span) = as_record(v) else { return };
    let name = field(span, "name").and_then(as_string).unwrap_or("?");
    let kind = field(span, "span-kind")
        .and_then(|k| {
            if let Val::Enum(s) = k {
                Some(s.as_str())
            } else {
                None
            }
        })
        .unwrap_or("?");
    println!("[OTEL/SPAN-END]   name={name} kind={kind}");
}

fn print_log_emit(v: &Val) {
    let Some(rec) = as_record(v) else { return };
    let severity = field(rec, "severity-text")
        .and_then(as_option_string)
        .unwrap_or("?");
    let body = field(rec, "body").and_then(as_option_string).unwrap_or("?");
    println!("[OTEL/LOG]        severity={severity} body={body}");
}

fn empty_span_context() -> Val {
    Val::Record(vec![
        ("trace-id".into(), Val::String(String::new())),
        ("span-id".into(), Val::String(String::new())),
        ("trace-flags".into(), Val::Flags(vec![])),
        ("is-remote".into(), Val::Bool(false)),
        ("trace-state".into(), Val::List(vec![])),
    ])
}

fn as_record(v: &Val) -> Option<&[(String, Val)]> {
    if let Val::Record(fs) = v {
        Some(fs)
    } else {
        None
    }
}

fn field<'a>(record: &'a [(String, Val)], name: &str) -> Option<&'a Val> {
    record.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

fn as_string(v: &Val) -> Option<&str> {
    if let Val::String(s) = v {
        Some(s.as_str())
    } else {
        None
    }
}

fn as_option_string(v: &Val) -> Option<&str> {
    if let Val::Option(Some(inner)) = v {
        as_string(inner)
    } else {
        None
    }
}

fn as_u64(v: &Val) -> Option<u64> {
    if let Val::U64(n) = v {
        Some(*n)
    } else {
        None
    }
}

fn debug_val(v: &Val) -> String {
    format!("{v:?}")
}
