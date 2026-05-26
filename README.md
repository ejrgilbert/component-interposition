# Wasi P3 `Service` + `Middleware` #

The purpose of this project is to provide a standalone example on how bundle a server+middleware into a single Wasm
component and run it on Wasmtime.

If interested, here's a live demonstration of this project as well: https://www.youtube.com/live/F0adyCd2RMs?si=cw4tIi5o33swh4gY&t=1039

Through following the instructions, a user should be able to get a fully working Wasm component that bundles a service
with the following structure (demo with `./run.sh`):

```
HTTP →
  M → Service
  M ← Response
← HTTP
```

This can be read as: "We have 1 middleware, M, that can do preprocessing on an HTTP request, then invoke the service with that preprocessed request.
Then, the same middleware, M, is returned to with the service's response. This response can be postprocessed by the middleware and returned."

This project also demonstrates how to interpose _N_ middlewares as in the following structure (demo with `./run.sh all --multiple`):
```
HTTP →
  M1 → M2 → ... → Mn → Service
  M1 ← M2 ← ... ← Mn ← Response
← HTTP
```

Further, let's say you have a component that already has been composed with service chaining, e.g.:
```
HTTP →
  S1 → S2
  S1 ← S2_response
← HTTP
```
If this is the case, interposition would need to occur in a way that _splices_ between the composed services, as in:
```
HTTP →
  S1 → M1 → ... → Mn → S2
  S1 ← M1 ← ... ← Mn ← S2_response
← HTTP
```
We have solved this problem for interposing 1 to N middleware(s) (demo with `./run.sh all --splice1`)  through leveraging
the [`splicer`](https://github.com/ejrgilbert/splicer) tool. This tool generalizes the application of interposition rules
to arbitrary component compositions through:
1. Discovering the composition of the component.
2. Planning how to splice the middleware in between the services that have already been composed based on some Yaml config.
3. Generating the `wac` script and command that performs the splice plan
4. Perform the `wac` operation to compose the service+middleware component.

# Interposition Works on Complex Compositions! #

## Nested Chain

The original topology (`./run.sh all --nested`):
```
[Service C] ──▶ [nested composition]
                         │
                         └──▶ [Service A] ──▶ [Service B]
```

After middleware injection (`./run.sh all --inner-nested1`):
```
[Service C] ──▶ [nested composition]
                         │
                         └──▶ [Service A] ── [Middleware a] ──▶ [Service B]
```

After middleware injection (`./run.sh all --pre-nested1`):
```
[Service C] ── [Middleware a] ──▶ [nested composition]
                                           │
                                           └──▶ [Service A] ──▶ [Service B]
```

After middleware injection (`./run.sh all --inner-pre-nested1`):
```
[Service C] ── [Middleware a] ──▶ [nested composition]
                                           │
                                           └──▶ [Service A] ── [Middleware b] ──▶ [Service B]
```

## "Fan In" Dependency ##

The original topology (`./run.sh all --fanin`):
```
   [Service A] ──┐
                 │
   [Service B] ──┼──▶ [Target Service]
                 │
   [Service C] ──┘
```

After middleware injection on a single downstream interface (`./run.sh all --fanin1):
```
   [Service A] ────[mdl-a]────┐
                              │
   [Service B] ───────────────┼───▶ [Target Service]
                              │
   [Service C] ───────────────┘
```

After middleware injection on ALL downstream interfaces (`./run.sh all --fanin-all1):
```
   [Service A] ────[mdl-a]────┐
                              │
   [Service B] ────[mdl-a]────┼───▶ [Target Service]
                              │
   [Service C] ────[mdl-a]────┘
```
 
# What is "middleware" in the realm of Wasi HTTP? #

At a high level, `wasi:http/middleware` is just a component that both exports a `handler.handle` and imports another handler.handle. Your middleware sits in between:

```
incoming request
    ↓
your handler.handle (middleware)
    ↓ (forward)
imported handler.handle (next service/middleware)
    ↓
response bubbles back
```

This is what the middleware world looks like in WIT:
```wit
world middleware {
    include service;
    import handler;
}
```

So implementing middleware is simply:
1. Export `handler.handle`
2. Import `handler.handle` (the downstream handler, which would either be the `service` itself or even another middleware!)
3. Do something before/after forwarding the request

Mental model:
- Middleware = handler that calls another handler
- "Import" = downstream
- "Export" = upstream

This world captures HTTP services that forward HTTP requests to another handler.
**Your middleware does not need to know what comes next — it just calls the imported `handler.handle`.**

# How to build #

The code to build, compose, and run the bundled service+middleware component is all inside the `run.sh` script.

Simply run it with no arguments to do the full workflow or pass arguments to execute each step separately, see `run.sh --help` for usage.

To help future people get their environment setup, these are the versions of tools I used for this to actually work (checked by `run.sh`):
- `cargo --version`: 1.93.0
- `wasm-tools --version`: 1.244.0
- `wkg --version`: 0.13.0
- `wac --version`: 0.9.0

## What is happening in this WAT?? ##

### The `service` (gets plugged in to the `middleware`) ###
Here's an explanation of the WAT you get from the compiled and componentized service (hopefully this helps!):
```webassembly
(component
    ;; This core module contains all of the actual logic of my service, but note below that it's
    ;; not directly used in the instantiation! This is because the core module exists only as a
    ;; _function provider_, not as a component instance.
    ;; Rather than instantiating this core module directly, we instantiate a component-level adapter (“shim”) whose job is to:
    ;; - import a component function (handle)
    ;; - canon-lift a core async ABI function into that component function
    ;; - re-export it with the exact WIT shape required by my:service/handler
    ;; See the shim explanation below.
    (core module ... )
    
    ;; -------------------------------------------------------------------------
    ;; The following ALIASes allow the core functions to be referenced later.
    ;; "Give me a handle to these two raw ABI functions so I can adapt them."
    
    ;; ALIAS to the entry function -- this starts the async operation and returns immediately.
    (alias core export $main "[async-lift]my:service/handler#handle"
        (core func $"[async-lift]my:service/handler#handle" (;58;)))
    ;; ALIAS to the callback function -- this is invoked later by the runtime to resume/complete the future.
    (alias core export $main "[callback][async-lift]my:service/handler#handle"
        (core func $"[callback][async-lift]my:service/handler#handle" (;59;)))
    
    ;; --------------------------------------------------------------------------
    ;; Now, start actually stitching together the SHIM component that uses the
    ;; (core module ...) above 
    ;; “Create a component function called $handle by wrapping the core async
    ;;  ABI function using the canonical ABI.”
    (func $handle (;15;) (type 28)
        (canon lift (core func $"[async-lift]my:service/handler#handle")
        (memory $memory)
        (realloc $cabi_realloc)
        string-encoding=utf8
        async
        (callback $"[callback][async-lift]my:service/handler#handle")))
    
    ;; This shim component just re-exports a function with the exact WIT signature required by the world.
    (component $wasi:http/handler@0.3.0-rc-2026-01-06-shim-component
        ...
        (import "import-func-handle" (func ...)) ;; this is INTERNALLY DEFINED by the core module and passed in on instantiation!
        ...
        (export "handle" (func 0))
    )
    
    ;; --------------------------------------------------------------------------
    ;; And now we actually instantiate the SHIM that we defined above with the correct world signature!
    (instance $wasi:http/handler@0.3.0-rc-2026-01-06-shim-instance (;11;) (instantiate $wasi:http/handler@0.3.0-rc-2026-01-06-shim-component
        ;; The lifted handle we pulled from the core module
        (with "import-func-handle" (func $handle))
        (with "import-type-request" (type $"#type29 request"))
        (with "import-type-response" (type $"#type30 response"))
        (with "import-type-error-code" (type $"#type31 error-code"))
        (with "import-type-request0" (type $request))
        (with "import-type-response0" (type $response))
        (with "import-type-error-code0" (type $error-code))
    ))
    
    ;; Export for the world :)
    (export $wasi:http/handler@0.3.0-rc-2026-01-06 (;12;) "wasi:http/handler@0.3.0-rc-2026-01-06" (instance $wasi:http/handler@0.3.0-rc-2026-01-06-shim-instance))
    ...
)
```

### The `middleware` (gets plugged in to the `service`) ###
Here's an explanation of the WAT you get from the compiled and componentized service (hopefully this helps!).

First, what the middleware WIT world means:
```wit
// A component that wraps another handler and re-exports a handler
// "I am both a client and a server."
world middleware {
  import types;
  import handler; // "Import the entire interface instance wasi:http/handler."
  export handler; // "I also implement and export the wasi:http/handler interface instance."
}
```

Now that we have that in our brains, what does the WAT mean?
```webassembly
(component
    ;; We get this because of `import handler;` in the WIT!
    ;; "Import the entire interface instance wasi:http/handler."
    ;; This can now be used to call the downstream `handler` function.
    (type $ty-wasi:http/handler@0.3.0-rc-2026-01-06 (;4;)
        (instance
            ...
            (export (;0;) "handle" (func (type 9)))
        ))
    (import "wasi:http/handler@0.3.0-rc-2026-01-06"
        (instance $wasi:http/handler@0.3.0-rc-2026-01-06 (;1;)
            (type $ty-wasi:http/handler@0.3.0-rc-2026-01-06)))

    ;; This core module implements the `middleware` logic.
    (core module
        ;; "Import the core ABI entrypoint for calling the downstream handler."
        ;; This is the middleware calling the next `handler` in the chain.
        (import "wasi:http/handler@0.3.0-rc-2026-01-06" "[async-lower]handle" (func (;1;) (type 2)))
        ;; This function exists because async requires task completion plumbing, it:
        ;; 1. completes the async task
        ;; 2. propagates the result back to the runtime
        ;; "When my middleware finishes, tell the executor the future is done."
        (import "[export]wasi:http/handler@0.3.0-rc-2026-01-06" "[task-return]handle" (func  (;4;) (type 7)))
        ...
    )
    
    ;; Instantiate middleware's core module and satisfy the `import handler;` with the import instance
    (core instance $main (;7;)
        (instantiate $main
            (with "wasi:http/handler@0.3.0-rc-2026-01-06" (instance $wasi:http/handler@0.3.0-rc-2026-01-06))
            (with "[export]wasi:http/handler@0.3.0-rc-2026-01-06" (instance $"[export]wasi:http/handler@0.3.0-rc-2026-01-06"))
            ...
        ))

    
    ;; -------------------------------------------------------------------------
    ;; The following ALIASes allow the core functions to be referenced later.
    ;; "Give me a handle to these two raw ABI functions so I can adapt them."
    ;; NOTE: These are from the CORE INSTANCE of the core module above!
   
    ;; ALIAS to the entry function -- this starts the async operation and returns immediately.
    (alias core export $main "[async-lift]wasi:http/handler@0.3.0-rc-2026-01-06#handle"
        (core func $"[async-lift]wasi:http/handler@0.3.0-rc-2026-01-06#handle" (;64;)))
    ;; ALIAS to the callback function -- this is invoked later by the runtime to resume/complete the future.
    (alias core export $main "[callback][async-lift]wasi:http/handler@0.3.0-rc-2026-01-06#handle"
        (core func $"[callback][async-lift]wasi:http/handler@0.3.0-rc-2026-01-06#handle" (;65;)))
    
    ;; --------------------------------------------------------------------------
    ;; Now, start actually stitching together the SHIM component that uses the
    ;; (core instance ...) above 
    ;; "Create a component function called $handle by wrapping the core async
    ;;  ABI function using the canonical ABI."
    (func $"#func16 handle" (@name "handle") (;16;) (type 36)
        (canon lift (core func $"[async-lift]wasi:http/handler@0.3.0-rc-2026-01-06#handle")
        (memory $memory)
        string-encoding=utf8 async
        (callback $"[callback][async-lift]wasi:http/handler@0.3.0-rc-2026-01-06#handle")))
    
    ;; This shim component just re-exports a function with the exact WIT signature required by the world.
    (component $wasi:http/handler@0.3.0-rc-2026-01-06-shim-component
        ...
        (import "import-func-handle" (func (;0;) ...)) ;; this is INTERNALLY DEFINED by the core module instance and passed in on instantiation!
        ...
        (export "handle" (func 0))
    )
    
    ;; --------------------------------------------------------------------------
    ;; And now we actually instantiate the SHIM that we defined above with the correct world signature!
    (instance $wasi:http/handler@0.3.0-rc-2026-01-06-shim-instance (;12;) (instantiate $wasi:http/handler@0.3.0-rc-2026-01-06-shim-component
        ;; The lifted handle we pulled from the core module instance
        (with "import-func-handle" (func $handle))
        (with "import-type-request" (type $"#type29 request"))
        (with "import-type-response" (type $"#type30 response"))
        (with "import-type-error-code" (type $"#type31 error-code"))
        (with "import-type-request0" (type $request))
        (with "import-type-response0" (type $response))
        (with "import-type-error-code0" (type $error-code))
    ))
    
    ;; Export for the world :)
    (export $"#instance13 wasi:http/handler@0.3.0-rc-2026-01-06" (@name "wasi:http/handler@0.3.0-rc-2026-01-06") (;13;) "wasi:http/handler@0.3.0-rc-2026-01-06"
        (instance $wasi:http/handler@0.3.0-rc-2026-01-06-shim-instance))
    ...
)
```

### The full composition of the `middleware`+`service` ###
```webassembly
(component
    ...
    
    ;; The only part to really pay attention to is at the bottom of the component.
    ;; This is the meat of what is accomplished with the `wac` script.
    
    ;; Create an instance that we plug the handler from the MIDDLEWARE into
    (instance $mdl (;13;) (instantiate 1
          (with "wasi:http/handler@0.3.0-rc-2026-01-06" (instance 12))  ;; points to the middleware instance
          (with "wasi:cli/environment@0.2.6" (instance 1))
          (with "wasi:cli/exit@0.2.6" (instance 2))
          (with "wasi:io/error@0.2.6" (instance 3))
          (with "wasi:io/streams@0.2.6" (instance 4))
          (with "wasi:clocks/wall-clock@0.2.6" (instance 8))
          (with "wasi:filesystem/types@0.2.6" (instance 9))
          (with "wasi:filesystem/preopens@0.2.6" (instance 10))
          (with "wasi:cli/stderr@0.2.6" (instance 7))
          (with "wasi:cli/stdin@0.2.6" (instance 5))
          (with "wasi:cli/stdout@0.2.6" (instance 6))
          (with "wasi:http/types@0.3.0-rc-2026-01-06" (instance 0))
        ))
        
    ;; Just allows this instance to be referred to in the export that follows.
    (alias export $mdl "wasi:http/handler@0.3.0-rc-2026-01-06" (instance (;14;)))
    
    ;; Explicitly export the handler defined provided by the middleware instance that's
    ;; now been stitched together appropriately with the core service above!
    ;; Now we have the appropriate shape of a `service` world :)
    (export (;15;) "wasi:http/handler@0.3.0-rc-2026-01-06" (instance 14))
)
```
