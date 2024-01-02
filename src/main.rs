use std::collections::HashMap;

use axum::{
    http::{StatusCode, Uri},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use once_cell::sync::Lazy;
use rand::distributions::{Alphanumeric, DistString};
use serde::Deserialize;
use tokio::{net::TcpListener, sync::RwLock};

static ROUTES: Lazy<RwLock<HashMap<String, String>>> = Lazy::new(|| RwLock::new(HashMap::new()));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/register", post(register))
        .route("/list", get(list))
        .fallback(redirect);

    let socket_addr = std::env::var("SOCKET_ADDR").unwrap_or("0.0.0.0:5000".into());
    let listener = TcpListener::bind(socket_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct RegistrationInfo {
    target: String,
    name: Option<String>,
}

async fn register(Json(info): Json<RegistrationInfo>) -> impl IntoResponse {
    let short = match info.name {
        Some(name) => name,
        None => match random_short().await {
            Some(random) => random,
            None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
    };

    ROUTES
        .write()
        .await
        .insert(short.clone(), info.target.clone());

    short.into_response()
}

async fn redirect(uri: Uri) -> impl IntoResponse {
    let short = &uri.path()[1..];

    match ROUTES.read().await.get(short) {
        Some(r) => Redirect::to(r).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
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
