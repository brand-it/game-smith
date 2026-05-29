---
name: loco-errors
description: Error handling conventions for Loco.rs controllers — StandardError type, tracing patterns, and error response rendering
---

# Loco Error Handling

Standardizes controller error responses in this Loco project. Use this skill when creating new controllers, handling errors, or working with the `StandardError` type.

## StandardError

Controllers return `Result<impl IntoResponse, StandardError>` instead of `loco_rs::Result<impl IntoResponse>`. The `StandardError` enum lives in `src/controllers/error.rs` and provides styled HTML error pages matching the app's sidebar layout.

### Variants

| Variant | HTTP Status | Use Case |
|---------|-------------|----------|
| `InternalServerError(msg)` | 500 | Unexpected errors, database failures |
| `NotFound(msg)` | 404 | Missing resources |
| `BadRequest(msg)` | 400 | Validation or input errors |
| `Unauthorized(msg)` | 401 | Auth failures |

### Example Controller

```rust
use crate::controllers::error::StandardError;

pub async fn show(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| StandardError::InternalServerError(format!("failed to find game server: {e}")))?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;
    Ok(crate::views::game_servers::show(&ctx, v, &server).await?)
}
```

### Mapping Errors

Map domain errors to `StandardError` at the controller boundary:

```rust
installer
    .start(&server)
    .await
    .map_err(|e| StandardError::InternalServerError(format!("failed to start server: {e}")))?;
```

### View Functions Returning loco_rs::Error

View functions return `Result<impl IntoResponse, loco_rs::Error>`. Wrap them with `Ok(...?)` to convert:

```rust
Ok(crate::views::game_servers::list(&ctx, v, &servers).await?)
```

This works because `StandardError` implements `From<loco_rs::Error>`.

## Error Logging

`StandardError` logs before rendering:
- `InternalServerError` → `tracing::error!`
- `NotFound`, `BadRequest`, `Unauthorized` → `tracing::warn!`

The log includes `error.msg` for operators and `http.method`, `http.uri`, `request_id` from the request context.

## Converting loco_rs::Error to StandardError

`StandardError` implements `From<loco_rs::Error>`:

| `loco_rs::Error` | Converts to |
|-------------------|-------------|
| `Error::NotFound` | `StandardError::NotFound` |
| `Error::Unauthorized(msg)` | `StandardError::Unauthorized` |
| `Error::BadRequest(msg)` | `StandardError::BadRequest` |
| Everything else | `StandardError::InternalServerError` |

## Tracing Patterns

Loco uses structured tracing. Error events include:

| Field | Purpose |
|-------|---------|
| `error.msg` | Human-readable message |
| `http.method`, `http.uri` | Request context |
| `request_id` | Correlates log lines for a single request |

### Logging at the domain level

Use `tracing::warn!` with contextual fields before returning errors:

```rust
warn!(
    server_id = server.id,
    install_dir = %server.install_dir,
    "No server executable found and no boot script configured"
);
```

## Key Constraints

1. **Controllers return `StandardError`** — never `loco_rs::Error` directly in controller signatures.
2. **View functions return `loco_rs::Error`** — they are lower-level and don't need styled HTML.
3. **Errors are sanitized for clients** — the HTML error page shows a generic message; details go to logs only.
4. **`Ok(...?)` pattern** — wrap view function calls in `Ok(...?)` to bridge `loco_rs::Error` to `StandardError`.
