use axum::http::StatusCode;
use axum::response::IntoResponse;

/// Custom error type with styled HTML rendering.
///
/// Replaces loco's generic JSON error responses with formatted HTML error
/// pages that match the app's sidebar layout and use Tailwind CSS.
///
/// Controllers return `Result<impl IntoResponse, StandardError>` — errors are
/// automatically rendered as styled HTML pages without explicit error handling
/// in each handler.
#[derive(Debug)]
pub enum StandardError {
    InternalServerError(String),
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
}

impl StandardError {
    /// Log the error before rendering.
    fn log(&self) {
        match self {
            Self::InternalServerError(msg) => {
                tracing::error!(error.msg = %msg, "Internal Server Error");
            }
            Self::NotFound(msg) => {
                tracing::warn!(error.msg = %msg, "Not Found");
            }
            Self::BadRequest(msg) => {
                tracing::warn!(error.msg = %msg, "Bad Request");
            }
            Self::Unauthorized(msg) => {
                tracing::warn!(error.msg = %msg, "Unauthorized");
            }
        }
    }
}

impl std::fmt::Display for StandardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InternalServerError(msg) => write!(f, "Internal Server Error: {msg}"),
            Self::NotFound(msg) => write!(f, "Not Found: {msg}"),
            Self::BadRequest(msg) => write!(f, "Bad Request: {msg}"),
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {msg}"),
        }
    }
}

impl std::error::Error for StandardError {}

impl IntoResponse for StandardError {
    #[allow(clippy::too_many_lines)]
    fn into_response(self) -> axum::response::Response {
        self.log();
        let (status, title, message) = match self {
            Self::InternalServerError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
                msg,
            ),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "Not Found", msg),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "Bad Request", msg),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "Unauthorized", msg),
        };

        let logs_path = crate::AppDirs::new(crate::resolve_data_home())
            .logs_dir
            .to_string_lossy()
            .into_owned();
        let body = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} — Game Smith</title>
    <link rel="stylesheet" href="/static/css/tailwind.css">
    <style>
        html, body {{ height: 100%; margin: 0; }}
        .layout {{ display: flex; min-height: 100vh; }}
        .sidebar {{
            width: 220px;
            min-height: 100vh;
            background: #1e293b;
            display: flex;
            flex-direction: column;
            flex-shrink: 0;
        }}
        .sidebar-brand {{
            padding: 20px 20px 16px;
            font-size: 1rem;
            font-weight: 700;
            color: #fff;
            border-bottom: 1px solid #334155;
            letter-spacing: -0.01em;
        }}
        .sidebar-nav {{
            flex: 1;
            padding: 12px 8px;
            display: flex;
            flex-direction: column;
            gap: 2px;
        }}
        .sidebar-nav a {{
            display: flex;
            align-items: center;
            gap: 10px;
            padding: 8px 12px;
            border-radius: 6px;
            font-size: 0.875rem;
            font-weight: 500;
            color: #94a3b8;
            text-decoration: none;
            transition: background 0.1s, color 0.1s;
        }}
        .sidebar-nav a:hover {{
            background: #334155;
            color: #f1f5f9;
        }}
        .sidebar-nav a.active {{
            background: #334155;
            color: #f1f5f9;
        }}
        .sidebar-nav a svg {{
            width: 16px;
            height: 16px;
            flex-shrink: 0;
            opacity: 0.7;
        }}
        .sidebar-nav a:hover svg,
        .sidebar-nav a.active svg {{
            opacity: 1;
        }}
        .sidebar-footer {{
            padding: 12px 20px;
            font-size: 0.75rem;
            color: #475569;
            border-top: 1px solid #334155;
        }}
        .main {{
            flex: 1;
            background: #f8fafc;
            min-height: 100vh;
        }}
        .main-inner {{
            max-width: 1024px;
            margin: 0 auto;
            padding: 40px 32px;
        }}
    </style>
</head>
<body>
<div class="layout">

    <!-- Sidebar -->
    <aside class="sidebar">
        <div class="sidebar-brand">Game Smith</div>
        <nav class="sidebar-nav">
            <a href="/">
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6"/></svg>
                Dashboard
            </a>
            <a href="/servers">
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"/></svg>
                Servers
            </a>
            <a href="/steamcmd">
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94 1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>
                SteamCMD
            </a>
            <a href="/commands">
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"/></svg>
                Commands
            </a>
            <a href="/steam-config">
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/></svg>
                Steam Config
            </a>
        </nav>
        <div class="sidebar-footer">v0.1.0</div>
    </aside>

    <!-- Main content -->
    <div class="main">
        <div class="main-inner">
            <div style="text-align:center;padding:80px 0;">
                <div style="font-size:6rem;font-weight:800;color:#1e293b;line-height:1;margin-bottom:16px;">{status}</div>
                <h1 style="font-size:1.5rem;font-weight:600;color:#1e293b;margin:0 0 8px;">{title}</h1>
                <p style="font-size:1rem;color:#475569;margin:0 0 24px;">{message}</p>
                <p style="font-size:0.875rem;color:#94a3b8;margin:0;">Check the logs at <code style="background:#e2e8f0;padding:2px 6px;border-radius:4px;">{logs_path}</code> for more details.</p>
            </div>
        </div>
    </div>

</div>

<script>
(function () {{
    var path = location.pathname;
    document.querySelectorAll(".sidebar-nav a").forEach(function (link) {{
        var href = link.getAttribute("href");
        var exact = link.hasAttribute("data-exact");
        var active = exact ? path === href : path === href || path.startsWith(href + "/");
        if (active) link.classList.add("active");
    }});
}})();
</script>
</body>
</html>"#,
            status = status.as_u16(),
            title = html_escape(title),
            message = html_escape(&message),
        );

        (status, axum::response::Html(body)).into_response()
    }
}

impl From<loco_rs::Error> for StandardError {
    fn from(err: loco_rs::Error) -> Self {
        match err {
            loco_rs::Error::NotFound => Self::NotFound("Resource not found".into()),
            loco_rs::Error::Unauthorized(msg) => Self::Unauthorized(msg),
            loco_rs::Error::BadRequest(msg) => Self::BadRequest(msg),
            _ => Self::InternalServerError(err.to_string()),
        }
    }
}

/// Minimal HTML escape to prevent XSS in error messages.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}
