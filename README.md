# Services with Middleware on Wasi P3 #

To help future people get their environment setup, these are the versions of tools I used for this to actually work:
- `cargo --version`: 1.93.0
- `wasm-tools --version`: 1.244.0

TODO: 
- [ ] Get basic composition working
- [ ] Document everything
- [ ] Figure out how to inherit `Cargo.toml` from base of project

# How to build #

Build the `service`:
```shell
pushd service

# Pull in wit dependencies
wkg wit fetch

# Build
cargo build --target wasm32-wasip1
SVC_PTH_MOD="./target/wasm32-wasip1/debug/service.wasm"
SVC_PTH_MOD_WAT="./target/wasm32-wasip1/debug/service.wat"

# Check the WAT (should be a MODULE)
ls -al $SVC_PTH_MOD
wasm-tools print $SVC_PTH_MOD -o $SVC_PTH_MOD_WAT

# Convert to a component with the adapter
SVC_PTH_COMP="./target/wasm32-wasip1/debug/service.comp.wasm"
SVC_PTH_COMP_WAT="./target/wasm32-wasip1/debug/service.comp.wat"
ADAPTER_PTH="../wasi_snapshot_preview1.reactor.wasm"
wasm-tools component new $SVC_PTH_MOD --adapt $ADAPTER_PTH --skip-validation -o $SVC_PTH_COMP

# Check the WAT (should be a COMPONENT, see the "What is happening in this WAT??" section for details)
ls -al $SVC_PTH_COMP
wasm-tools print $SVC_PTH_COMP -o $SVC_PTH_COMP_WAT

popd
```

Build the `middleware`:
```shell
pushd middleware

# Pull in wit dependencies
wkg wit fetch

# Build
cargo build --target wasm32-wasip1
PTH_MOD="./target/wasm32-wasip1/debug/middleware.wasm"
PTH_MOD_WAT="./target/wasm32-wasip1/debug/middleware.wat"

# Check the WAT (should be a MODULE)
ls -al $PTH_MOD
wasm-tools print $PTH_MOD -o $PTH_MOD_WAT

# Convert to a component with the adapter
PTH_COMP="./target/wasm32-wasip1/debug/middleware.comp.wasm"
PTH_COMP_WAT="./target/wasm32-wasip1/debug/middleware.comp.wat"
ADAPTER_PTH="../wasi_snapshot_preview1.reactor.wasm"
wasm-tools component new $PTH_MOD --adapt $ADAPTER_PTH --skip-validation -o $PTH_COMP

# Check the WAT (should be a COMPONENT, see the "What is happening in this WAT??" section for details)
ls -al $PTH_COMP
wasm-tools print $PTH_COMP -o $PTH_COMP_WAT

popd

```

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
    (component $my:service/handler-shim-component
        ...
        (import "import-func-handle" (func ...)) ;; this is INTERNALLY DEFINED by the core module and passed in on instantiation!
        ...
        (export "handle" (func 0))
    )
    
    ;; --------------------------------------------------------------------------
    ;; And now we actually instantiate the SHIM that we defined above with the correct world signature!
    (instance $my:service/handler-shim-instance (;11;) (instantiate $my:service/handler-shim-component
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
    (export $my:service/handler (;12;) "my:service/handler" (instance $my:service/handler-shim-instance))
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
