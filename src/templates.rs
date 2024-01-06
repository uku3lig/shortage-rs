use askama::Template;
use axum::http::StatusCode;

pub const NOT_FOUND: (StatusCode, BaseTemplate) = (
    StatusCode::NOT_FOUND,
    BaseTemplate {
        content: "404 Not Found",
    },
);

pub const INTERNAL_SERVER_ERROR: (StatusCode, BaseTemplate) = (
    StatusCode::INTERNAL_SERVER_ERROR,
    BaseTemplate {
        content: "500 Internal Server Error",
    },
);

#[derive(Template)]
#[template(path = "base.html")]
pub struct BaseTemplate<'a> {
    pub content: &'a str,
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate<'a> {
    pub redirect_url: &'a str,
}

#[derive(Template)]
#[template(path = "registered.html")]
pub struct RegisteredTemplate {
    pub host: String,
    pub short: String,
}
