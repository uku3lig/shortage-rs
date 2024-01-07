use std::collections::HashMap;

use axum::{
    extract::Host,
    http::{StatusCode, Uri},
    response::{IntoResponse, Redirect},
    routing::post,
    Form, Router,
};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};

use crate::{
    auth::User,
    templates::{self, BaseTemplate, RegisteredTemplate},
    AuthSession,
};

static ROUTES: Lazy<Mutex<HashMap<String, ShortenedUrl>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize)]
pub struct ShortenedUrl {
    pub owner: u32,
    pub target: String,
    pub uses: usize,
    pub expiration: Option<DateTime<Utc>>,
    pub max_uses: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct RegistrationInfo {
    target: String,
    name: Option<String>,
    expiration: Option<String>, // RAFHHGHGHH I HATE HTML FORMS
    max_uses: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct DeleteInfo {
    name: String,
}

pub fn router() -> Router {
    Router::new()
        .route("/register", post(register))
        .route("/edit", post(edit))
        .route("/remove", post(remove))
}

async fn register(
    auth_session: AuthSession,
    Host(host): Host,
    Form(info): Form<RegistrationInfo>,
) -> impl IntoResponse {
    let Some(user) = auth_session.user else {
        tracing::error!("user was not authenticated :interrobang:");
        return templates::INTERNAL_SERVER_ERROR.into_response();
    };

    let expiration = match parse_date(info.expiration) {
        Ok(exp) => exp,
        Err(e) => return e.into_response(),
    };

    let short = match info.name {
        Some(name) => name,
        None => match random_short().await {
            Some(random) => random,
            None => return templates::INTERNAL_SERVER_ERROR.into_response(),
        },
    };

    let shortened = ShortenedUrl {
        owner: user.id,
        target: info.target,
        uses: 0,
        expiration,
        max_uses: info.max_uses,
    };

    ROUTES.lock().insert(short.clone(), shortened);

    RegisteredTemplate { host, short }.into_response()
}

async fn edit(auth_session: AuthSession, Form(info): Form<RegistrationInfo>) -> impl IntoResponse {
    let Some(user) = auth_session.user else {
        tracing::error!("user was not authenticated :interrobang:");
        return templates::INTERNAL_SERVER_ERROR.into_response();
    };

    let Some(name) = info.name else {
        return (StatusCode::UNPROCESSABLE_ENTITY, "field `name` is required").into_response();
    };

    let expiration = match parse_date(info.expiration) {
        Ok(exp) => exp,
        Err(e) => return e.into_response(),
    };

    match ROUTES.lock().get_mut(&name) {
        Some(url) if url.owner == user.id => {
            *url = ShortenedUrl {
                owner: url.owner,
                target: info.target,
                uses: url.uses,
                expiration,
                max_uses: info.max_uses,
            };

            BaseTemplate {
                content: "Sucessfully edited!",
            }
            .into_response()
        }
        _ => templates::NOT_FOUND.into_response(),
    }
}

async fn remove(auth_session: AuthSession, Form(info): Form<DeleteInfo>) -> impl IntoResponse {
    let Some(user) = auth_session.user else {
        tracing::error!("user was not authenticated :interrobang:");
        return templates::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut routes = ROUTES.lock();

    match routes.get(&info.name) {
        Some(url) if url.owner == user.id => {
            routes.remove(&info.name);

            BaseTemplate {
                content: "Sucessfully removed!",
            }
            .into_response()
        }
        _ => templates::NOT_FOUND.into_response(),
    }
}

pub fn list(user: &User) -> Vec<ShortenedUrl> {
    let routes = ROUTES.lock();

    let routes = routes
        .iter()
        .map(|(_, u)| u)
        .filter(|url| url.owner == user.id)
        .cloned()
        .collect::<Vec<_>>();

    routes
}

pub async fn redirect(uri: Uri) -> impl IntoResponse {
    let short = &uri.path()[1..];
    let mut routes = ROUTES.lock();

    if let Some(u) = routes.get_mut(short) {
        u.uses += 1;

        if u.expiration.filter(|&t| t < Utc::now()).is_some()
            || u.max_uses.filter(|&m| u.uses > m).is_some()
        {
            routes.remove(short);
        } else {
            return Redirect::to(&u.target).into_response();
        }
    }

    templates::NOT_FOUND.into_response()
}

async fn random_short() -> Option<String> {
    let routes = ROUTES.lock();

    // if this fails a whole TEN TIMES, i'm going to buy a lottery ticket
    std::iter::from_fn(|| Some(Alphanumeric.sample_string(&mut rand::thread_rng(), 8)))
        .take(10)
        .find(|s| !routes.contains_key(s))
}

fn parse_date(expiration: Option<String>) -> Result<Option<DateTime<Utc>>, impl IntoResponse> {
    if let Some(mut expiration) = expiration {
        expiration.push_str(":00Z"); // i want to die

        DateTime::parse_from_rfc3339(&expiration)
            .map(|d| Some(d.with_timezone(&Utc)))
            .map_err(|e| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("Could not parse expiration: {e}"),
                )
            })
    } else {
        Ok(None)
    }
}
