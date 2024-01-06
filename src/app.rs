use std::collections::HashMap;

use axum::{
    extract::{Host, Path},
    http::{StatusCode, Uri},
    response::{IntoResponse, Redirect},
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};

use crate::{
    templates::{self, BaseTemplate, RegisteredTemplate},
    AuthSession,
};

static ROUTES: Lazy<Mutex<HashMap<String, ShortenedUrl>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Serialize)]
struct ShortenedUrl {
    owner: u32,
    target: String,
    uses: usize,
    expiration: Option<DateTime<Utc>>,
    max_uses: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct RegistrationInfo {
    target: String,
    name: Option<String>,
    expiration: Option<DateTime<Utc>>,
    max_uses: Option<usize>,
}

pub fn router() -> Router {
    Router::new()
        .route("/register", post(register))
        .route("/edit", patch(edit))
        .route("/remove/:name", delete(remove))
        .route("/list", get(list))
}

async fn register(
    auth_session: AuthSession,
    Host(host): Host,
    Json(info): Json<RegistrationInfo>,
) -> impl IntoResponse {
    let Some(user) = auth_session.user else {
        tracing::error!("user was not authenticated :interrobang:");
        return templates::INTERNAL_SERVER_ERROR.into_response();
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
        expiration: info.expiration,
        max_uses: info.max_uses,
    };

    ROUTES.lock().insert(short.clone(), shortened);

    RegisteredTemplate { host, short }.into_response()
}

async fn edit(auth_session: AuthSession, Json(info): Json<RegistrationInfo>) -> impl IntoResponse {
    let Some(user) = auth_session.user else {
        tracing::error!("user was not authenticated :interrobang:");
        return templates::INTERNAL_SERVER_ERROR.into_response();
    };

    let Some(name) = info.name else {
        return (StatusCode::UNPROCESSABLE_ENTITY, "field `name` is required").into_response();
    };

    match ROUTES.lock().get_mut(&name) {
        Some(url) if url.owner == user.id => {
            *url = ShortenedUrl {
                owner: url.owner,
                target: info.target,
                uses: url.uses,
                expiration: info.expiration,
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

async fn remove(auth_session: AuthSession, Path(name): Path<String>) -> impl IntoResponse {
    let Some(user) = auth_session.user else {
        tracing::error!("user was not authenticated :interrobang:");
        return templates::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut routes = ROUTES.lock();

    match routes.get(&name) {
        Some(url) if url.owner == user.id => {
            routes.remove(&name);

            BaseTemplate {
                content: "Sucessfully removed!",
            }
            .into_response()
        }
        _ => templates::NOT_FOUND.into_response(),
    }
}

async fn list(auth_session: AuthSession) -> impl IntoResponse {
    let Some(user) = auth_session.user else {
        tracing::error!("user was not authenticated :interrobang:");
        return templates::INTERNAL_SERVER_ERROR.into_response();
    };

    let routes = ROUTES.lock();

    let routes = routes
        .iter()
        .filter(|(_, url)| url.owner == user.id)
        .collect::<HashMap<_, _>>();

    Json(routes).into_response()
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
