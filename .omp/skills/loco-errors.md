# Loco Errors and Pluggability

## Overview

Loco uses tracing for all error logging. Controller errors are sanitized for end-users (security) but logged with full detail. The framework provides hooks for custom error handling via middlewares, initializers, and logger configuration.

## Error Configuration

### Logger Settings (`config/*.yaml`)

```yaml
logger:
  enable: true
  pretty_backtrace: true  # RUST_BACKTRACE=1 — dev only
  level: debug            # trace, debug, info, warn, error
  format: compact         # compact, pretty, json
```

### Controller Logger Middleware

```yaml
server:
  middlewares:
    logger:
      enable: true  # request IDs, latency, status codes, error context
```

### Database Query Logging

```yaml
database:
  enable_logging: false  # log live SQL queries
```

## Error Output Fields

Loco errors appear as structured tracing events:

```
2024-02-xxT12:19:25.295954Z ERROR http-request: loco_rs::controller: controller_error \
  error.msg=invalid type: string "foo", expected a sequence \
  error.details=JSON(Error("...", line: 0, column: 0)) \
  http.method=GET http.uri=/notes \
  http.version=HTTP/1.1 \
  http.user_agent=curl/8.1.2 \
  environment=development \
  request_id=8622e624-9bda-49ce-9730-876f2a8a9a46
```

| Field | Purpose |
|-------|---------|
| `error.msg` | `to_string()` version — for operators |
| `error.details` | `Debug` representation — for developers |
| `controller_error` | Primary message tag — for searching/filtering |
| `http.method`, `http.uri` | Request context |
| `request_id` | Correlates all log lines for a single request |

## Producing Errors in Controllers

Controllers return `Result<impl IntoResponse>` where `Result` is `loco_rs::Result` (error type: `loco_rs::Error`):

```rust
use loco_rs::prelude::*;

// Custom error message
Err(Error::string("some custom message"))

// Convert another error to string
Err(Error::msg(other_error))

// Wrap another error with context
Err(Error::wrap(other_error))

// HTTP 401 Unauthorized
Err(Error::Unauthorized("some message"))

// Or use controller helpers (returns full Response, not Err)
unauthorized("some message")
```

### Loco Error Variants

The `loco_rs::Error` enum provides these constructors:

| Constructor | HTTP Status | Use Case |
|-------------|-------------|----------|
| `Error::string(msg)` | 500 | Internal errors |
| `Error::msg(err)` | 500 | Wrap existing error |
| `Error::wrap(err)` | 500 | Add context to error |
| `Error::Unauthorized(msg)` | 401 | Auth failures |
| `Error::BadRequest(msg)` | 400 | Validation/input errors |
| `Error::NotFound()` | 404 | Missing resources |
| `Error::CustomError(status, data)` | any | Custom status + JSON body |
| `Error::WithBacktrace { inner, backtrace }` | 400 | Errors with stack trace |

## Error Response Sanitization

Loco's `impl IntoResponse for Error` always returns sanitized JSON to the client. The actual error is logged via tracing but never exposed:

```json
{"error": "internal_server_error", "description": "Internal Server Error"}
```

To surface actual error messages to the browser, you must intercept at the middleware level — loco's error handler middleware is opaque.

## Customizing the Logger

Override `init_logger` in `src/app.rs` to control the tracing stack:

```rust
use loco_rs::prelude::*;

impl Hooks for App {
    fn init_logger(_config: &config::Config, _env: &Environment) -> Result<bool> {
        // Return Ok(true) if you took over logger initialization.
        // Return Ok(false) to use Loco's default logger.
        Ok(false)
    }
}
```

When returning `Ok(true)`, you are responsible for setting up the entire tracing subscriber stack. You cannot modify Loco's logger after initialization — tracing does not allow re-initialization.

## Initializers

Initializers wire infrastructure into the running app. They implement `Initializer` with two hooks:

- `before_run` — pure initialization (pre-flight checks, cache loading, cleanup)
- `after_routes` — modify the Axum router (add layers, middlewares)

### Creating an Initializer

```rust
// src/initializers/my_initializer.rs
use async_trait::async_trait;
use loco_rs::{prelude::*, Result};

pub struct MyInitializer;

#[async_trait]
impl Initializer for MyInitializer {
    fn name(&self) -> String {
        "my-initializer".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        // One-time initialization
        Ok(())
    }

    async fn after_routes(&self, router: AxumRouter, ctx: &AppContext) -> Result<AxumRouter> {
        // Add middleware layers
        Ok(router)
    }
}
```

### Registering Initializers

In `src/app.rs`:

```rust
async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
    Ok(vec![
        Box::new(initializers::my::MyInitializer),
        // ...
    ])
}
```

Order matters — initializers run in vec order. No implicit initializers exist.

### Adding Middleware via Initializer

```rust
use tower::Layer;

async fn after_routes(&self, router: AxumRouter, _ctx: &AppContext) -> Result<AxumRouter> {
    let router = router.layer(MyMiddlewareLayer);
    Ok(router)
}
```

## Middleware

### Basic Middleware (Tower Service)

```rust
use std::{convert::Infallible, task::{Context, Poll}};
use axum::{body::Body, extract::Request, response::Response};
use futures_util::future::BoxFuture;
use tower::{Layer, Service};

#[derive(Clone)]
pub struct MyLayer;

impl MyLayer {
    pub fn new() -> Self { Self }
}

impl<S> Layer<S> for MyLayer {
    type Service = MyService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        MyService { inner }
    }
}

#[derive(Clone)]
pub struct MyService<S> { inner: S }

impl<S, B> Service<Request<B>> for MyService<S>
where
    S: Service<Request<B>, Response = Response<Body>, Error = Infallible>
      + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            // pre-processing
            let (parts, body) = req.into_parts();
            tracing::info!("Request: {:?} {:?}", parts.method, parts.uri.path());
            let req = Request::from_parts(parts, body);
            inner.call(req).await
        })
    }
}
```

**Critical**: Always use `std::mem::replace` when cloning services to preserve readiness state.

### Adding Middleware to Routes

**At the route level:**
```rust
// src/app.rs
fn routes(_ctx: &AppContext) -> AppRoutes {
    AppRoutes::with_default_routes()
        .add_route(controllers::auth::routes().layer(MyLayer::new()))
}
```

**At the handler level:**
```rust
// src/controllers/auth.rs
pub fn routes() -> Routes {
    Routes::new()
        .prefix("auth")
        .add("/register", post(register).layer(MyLayer::new()))
}
```

### Middleware with AppContext

```rust
#[derive(Clone)]
pub struct MyLayer { state: AppContext }

impl MyLayer {
    pub fn new(state: AppContext) -> Self { Self { state } }
}

impl<S> Layer<S> for MyLayer {
    type Service = MyService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        MyService { inner, state: self.state.clone() }
    }
}

#[derive(Clone)]
pub struct MyService<S> { inner: S, state: AppContext }
```

## SharedStore

`AppContext.shared_store` is a type-safe, thread-safe heterogeneous storage for arbitrary types (`'static + Send + Sync`).

### Inserting Data

```rust
// In src/app.rs — after_context hook
async fn after_context(mut ctx: AppContext) -> Result<AppContext> {
    ctx.shared_store.insert(MyService { api_key: "key".to_string() });
    Ok(ctx)
}
```

### Retrieving Clone-able Types

```rust
#[axum::debug_handler]
pub async fn index(
    SharedStore(service): SharedStore<MyService>,  // auto-clones
) -> impl IntoResponse {
    format::empty()
}
```

### Retrieving Non-Clone Types

```rust
#[axum::debug_handler]
pub async fn index(
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    let service = ctx.shared_store.get_ref::<MyService>()
        .ok_or_else(|| Error::InternalServerError)?;
    format::empty()
}
```

## Key Constraints

1. **Logger is global** — you get one chance to set up tracing. Override `init_logger` and return `Ok(true)` to take control.
2. **Error responses are sanitized** — loco's error middleware returns generic JSON to clients. The real error is only in logs.
3. **Service cloning requires `std::mem::replace`** — cloned services do not inherit readiness state.
4. **Initializer ordering is explicit** — no implicit initializers. Vec order determines execution order.
5. **`after_routes` is the primary integration point** — this is where you add middleware layers to the router.
