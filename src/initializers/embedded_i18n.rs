use async_trait::async_trait;
use axum::Router as AxumRouter;
use fluent_templates::{static_loader, FluentLoader};
use include_dir::{include_dir, Dir, DirEntry};
use loco_rs::{
    app::{AppContext, Initializer},
    controller::views::{ViewEngine, ViewRenderer},
    Result,
};
use serde::Serialize as SerdeSerialize;
use tracing::info;

static_loader! {
    static LOCALES = {
        locales: "assets/i18n",
        fallback_language: "en-US",
        core_locales: "assets/i18n/_shared.ftl",
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

const VIEWS_DIR: Dir<'_> = include_dir!("assets/views");

/// Build a `tera::Tera` instance from embedded views.
///
/// Iterates over the compiled-in directory tree, strips the
/// `assets/views/` prefix so template names match what controllers
/// reference (e.g. `"commands/list.html"`), and registers each file
/// via `add_raw_template`.
fn build_tera() -> Result<tera::Tera> {
    let mut tera = tera::Tera::default();
    register_templates(&mut tera, &VIEWS_DIR)?;
    Ok(tera)
}

/// Recursively register all template files from an embedded directory.
fn register_templates(tera: &mut tera::Tera, dir: &Dir<'_>) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            DirEntry::File(file) => {
                let path = file.path().to_string_lossy().to_string();
                let name = path
                    .strip_prefix("assets/views/")
                    .unwrap_or(&path)
                    .to_string();
                let content = std::str::from_utf8(file.contents()).map_err(|e| {
                    loco_rs::Error::string(&format!("invalid UTF-8 in template {name}: {e}"))
                })?;
                tera.add_raw_template(&name, content).map_err(|e| {
                    loco_rs::Error::string(&format!("failed to register {name}: {e}"))
                })?;
            }
            DirEntry::Dir(subdir) => {
                register_templates(tera, subdir)?;
            }
        }
    }
    Ok(())
}

/// Embedded views engine — wraps a [`tera::Tera`] instance built from
/// compile-time included templates.
///
/// Replaces `TeraView` (which reads templates from disk) so the
/// application runs without an `assets/views/` directory on the
/// target filesystem (RPM installs, single-binary deployments, etc.).
#[derive(Clone)]
pub struct EmbeddedViews {
    tera: tera::Tera,
}

impl EmbeddedViews {
    /// Build an [`EmbeddedViews`] instance from embedded templates with
    /// custom functions registered (`t` for i18n and `steamcmd_health`).
    ///
    /// # Errors
    /// Returns an error if template registration or function registration fails.
    pub fn build() -> Result<Self> {
        let mut tera = build_tera()?;

        // Register the same custom functions as TeraView::build().post_process()
        tera.register_function("t", FluentLoader::new(&*LOCALES));
        tera.register_function(
            "steamcmd_health",
            crate::data::steamcmd::tera_steamcmd_health,
        );

        // Also register loco-rs built-in tera filters for compatibility
        loco_rs::controller::views::tera_builtins::filters::register_filters(&mut tera);

        Ok(Self { tera })
    }
}

impl ViewRenderer for EmbeddedViews {
    fn render<S: SerdeSerialize>(&self, key: &str, data: S) -> Result<String> {
        let context = tera::Context::from_serialize(data)?;
        self.tera
            .render(key, &context)
            .map_err(|e| loco_rs::Error::string(&format!("template render failed for {key}: {e}")))
    }
}

pub struct EmbeddedI18n;

#[async_trait]
impl Initializer for EmbeddedI18n {
    fn name(&self) -> String {
        "embedded-i18n".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, _ctx: &AppContext) -> Result<AxumRouter> {
        info!("embedded i18n loaded");

        let embedded_views = EmbeddedViews::build()?;

        Ok(router.layer(axum::Extension(ViewEngine::from(embedded_views))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_embedded_views_build() {
        let views = EmbeddedViews::build().expect("Failed to build EmbeddedViews");

        let result = views
            .render(
                "steamcmd/status.html",
                json!({
                    "binary_path": "/tmp/steamcmd",
                    "installed": true,
                    "health_status": "healthy",
                    "last_check_id": None::<i32>,
                    "last_check_status": None::<&str>,
                }),
            )
            .expect("Failed to render");

        assert!(
            result.contains("SteamCMD"),
            "Template should contain SteamCMD text"
        );
    }
}
