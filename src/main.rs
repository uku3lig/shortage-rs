mod app;
mod auth;
mod oauth;

use std::env;

use axum_login::{
    login_required,
    tower_sessions::{
        cookie::{time::Duration, SameSite},
        Expiry, MemoryStore, SessionManagerLayer,
    },
    AuthManagerLayerBuilder,
};
use once_cell::sync::Lazy;
use tokio::net::TcpListener;

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .user_agent(format!("uku3lig/shortage/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap()
});

type AuthSession = axum_login::AuthSession<auth::Backend>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let backend = auth::Backend::new()?;

    let session_store = MemoryStore::default(); // TODO this is not for real applications :cold_sweat:
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_same_site(SameSite::Lax) // Ensure we send the cookie from the OAuth redirect.
        .with_expiry(Expiry::OnInactivity(Duration::days(1)));

    let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

    let app = app::router()
        .route_layer(login_required!(auth::Backend, login_url = "/login"))
        .merge(oauth::router())
        .fallback(app::redirect)
        .layer(auth_layer);

    let socket_addr = std::env::var("SOCKET_ADDR").unwrap_or("0.0.0.0:5000".into());
    let listener = TcpListener::bind(socket_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
