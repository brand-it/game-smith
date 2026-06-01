use async_trait::async_trait;
use axum::{
    body::Body,
    http::{header, Request, Response},
    middleware::Next,
    Router as AxumRouter,
};
use include_dir::{include_dir, Dir};
use loco_rs::{
    app::{AppContext, Initializer},
    Result,
};
use mime_guess::from_path;

const STATIC_DIR: Dir<'_> = include_dir!("assets/static");

fn lookup_static_file(path: &str) -> Option<(Vec<u8>, String)> {
    let entry = STATIC_DIR.get_file(path).or_else(|| {
        if path.ends_with('/') {
            STATIC_DIR.get_file(format!("{path}index.html").as_str())
        } else {
            None
        }
    })?;
    let content = entry.contents();
    let mime = from_path(path)
        .first_raw()
        .map_or_else(|| "application/octet-stream".to_string(), String::from);
    Some((content.to_vec(), mime))
}

/// Middleware handler that serves static files from embedded assets.
///
/// Intercepts requests starting with `/static/` and returns the corresponding
/// file from the embedded directory tree. Non-matching requests are passed
/// through to the next handler.
///
/// # Panics
///
/// Panics if the MIME content type for a static file cannot be parsed into
/// a valid HTTP header value.
pub async fn serve_static_middleware(request: Request<Body>, next: Next) -> Response<Body> {
    let path = request.uri().path().to_string();

    if path.starts_with("/static/") {
        let clean_path = path.strip_prefix("/static/").unwrap_or(&path).to_string();
        if let Some((content, mime_type)) = lookup_static_file(&clean_path) {
            let mut response = Response::new(Body::from(content));
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, mime_type.parse().unwrap());
            return response;
        }
    }

    next.run(request).await
}

pub struct EmbeddedStatic;

#[async_trait]
impl Initializer for EmbeddedStatic {
    fn name(&self) -> String {
        "embedded-static".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, _ctx: &AppContext) -> Result<AxumRouter> {
        Ok(router.layer(axum::middleware::from_fn(serve_static_middleware)))
    }
}
