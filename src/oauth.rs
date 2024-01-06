use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use axum_login::tower_sessions::Session;
use oauth2::CsrfToken;
use serde::Deserialize;

use crate::{
    auth::Credentials,
    templates::{self, BaseTemplate, LoginTemplate},
    AuthSession,
};

pub const CSRF_STATE_KEY: &str = "oauth.csrf-state";
pub const NEXT_URL_KEY: &str = "oauth.next-url";

#[derive(Debug, Deserialize)]
struct OauthResponse {
    code: String,
    state: CsrfToken,
}

#[derive(Debug, Deserialize)]
struct NextUrl {
    next: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/login", get(login))
        .route("/login/callback", get(oauth_callback))
        .route("/logout", get(logout))
}

async fn login(
    auth_session: AuthSession,
    session: Session,
    Query(query): Query<NextUrl>,
) -> impl IntoResponse {
    let (url, csrf_token) = auth_session.backend.authorize_url();

    if let Err(e) = session.insert(CSRF_STATE_KEY, csrf_token.secret()).await {
        tracing::error!("failed to insert CSRF_STATE_KEY: {e}");
        return templates::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Err(e) = session.insert(NEXT_URL_KEY, query.next).await {
        tracing::error!("failed to insert NEXT_URL_KEY: {e}");
        return templates::INTERNAL_SERVER_ERROR.into_response();
    }

    LoginTemplate {
        redirect_url: url.as_str(),
    }
    .into_response()
}

async fn oauth_callback(
    mut auth_session: AuthSession,
    session: Session,
    Query(query): Query<OauthResponse>,
) -> impl IntoResponse {
    let Ok(Some(old_state)) = session.get(CSRF_STATE_KEY).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let creds = Credentials {
        code: query.code,
        old_state,
        new_state: query.state,
    };

    let user = match auth_session.authenticate(creds).await {
        Ok(Some(user)) => user,
        Ok(None) => return (StatusCode::UNAUTHORIZED, "invalid CSRF state").into_response(),
        Err(e) => {
            tracing::error!("authentication failed: {e}");
            return templates::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Err(e) = auth_session.login(&user).await {
        tracing::error!("login for user {} failed: {e}", user.username);
        return templates::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Ok(Some(next)) = session.remove::<String>(NEXT_URL_KEY).await {
        Redirect::to(&next).into_response()
    } else {
        Redirect::to("/").into_response()
    }
}

async fn logout(mut auth_session: AuthSession) -> impl IntoResponse {
    match auth_session.logout().await {
        Ok(_) => BaseTemplate {
            content: "Logged out.",
        }
        .into_response(),
        Err(_) => templates::INTERNAL_SERVER_ERROR.into_response(),
    }
}
