use std::collections::HashMap;

use axum::{
    extract::Path,
    http::{StatusCode, Uri},
    response::{IntoResponse, Redirect},
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use rand::distributions::{Alphanumeric, DistString};
use serde::Deserialize;
use tokio::{net::TcpListener, sync::RwLock};

static ROUTES: Lazy<RwLock<HashMap<String, ShortenedUrl>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/register", post(register))
        .route("/edit", patch(edit))
        .route("/remove/:name", delete(remove))
        .route("/list", get(list))
        .fallback(redirect);

    let socket_addr = std::env::var("SOCKET_ADDR").unwrap_or("0.0.0.0:5000".into());
    let listener = TcpListener::bind(socket_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug)]
struct ShortenedUrl {
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

async fn register(Json(info): Json<RegistrationInfo>) -> impl IntoResponse {
    let short = match info.name {
        Some(name) => name,
        None => match random_short().await {
            Some(random) => random,
            None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
    };

    let shortened = ShortenedUrl {
        target: info.target,
        uses: 0,
        expiration: info.expiration,
        max_uses: info.max_uses,
    };

    ROUTES.write().await.insert(short.clone(), shortened);

    short.into_response()
}

async fn edit(Json(info): Json<RegistrationInfo>) -> impl IntoResponse {
    let Some(name) = info.name else {
        return (StatusCode::UNPROCESSABLE_ENTITY, "field `name` is required").into_response();
    };

    match ROUTES.write().await.get_mut(&name) {
        Some(u) => {
            *u = ShortenedUrl {
                target: info.target,
                uses: u.uses,
                expiration: info.expiration,
                max_uses: info.max_uses,
            };

            StatusCode::OK.into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn remove(Path(name): Path<String>) -> impl IntoResponse {
    match ROUTES.write().await.remove(&name) {
        Some(_) => StatusCode::OK,
        None => StatusCode::NOT_FOUND,
    }
}

async fn redirect(uri: Uri) -> impl IntoResponse {
    let short = &uri.path()[1..];
    let mut routes = ROUTES.write().await;

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

    StatusCode::NOT_FOUND.into_response()
}

async fn list() -> impl IntoResponse {
    format!("{:#?}", ROUTES.read().await)
}

async fn random_short() -> Option<String> {
    let routes = ROUTES.read().await;

    // if this fails a whole TEN TIMES, i'm going to buy a lottery ticket
    std::iter::from_fn(|| Some(Alphanumeric.sample_string(&mut rand::thread_rng(), 8)))
        .take(10)
        .find(|s| !routes.contains_key(s))
}
