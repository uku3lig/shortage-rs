use std::{collections::HashMap, env};

use axum::async_trait;
use axum_login::{AuthUser, AuthnBackend, UserId};
use oauth2::{
    basic::{BasicClient, BasicRequestTokenError},
    reqwest::{async_http_client, AsyncHttpClientError},
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, TokenResponse, TokenUrl,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use reqwest::Url;
use serde::Deserialize;

static USERS: Lazy<Mutex<HashMap<u32, User>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone)]
pub struct User {
    pub id: u32,
    pub username: String,
    access_token: String,
}

impl AuthUser for User {
    type Id = u32;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.access_token.as_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct Credentials {
    pub code: String,
    pub old_state: CsrfToken,
    pub new_state: CsrfToken,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    OAuth2(#[from] BasicRequestTokenError<AsyncHttpClientError>),
}

#[derive(Debug, Deserialize)]
struct GitHubResponse {
    id: u32,
    login: String,
}

#[derive(Debug, Clone)]
pub struct Backend {
    client: BasicClient,
}

impl Backend {
    pub fn new() -> anyhow::Result<Self> {
        let client_id = env::var("GITHUB_CLIENT_ID")
            .map(ClientId::new)
            .expect("no GITHUB_CLIENT_ID found");

        let client_secret = env::var("GITHUB_CLIENT_SECRET")
            .map(ClientSecret::new)
            .expect("no GITHUB_CLIENT_SECRET found");

        let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())?;

        let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())?;

        let client = BasicClient::new(client_id, Some(client_secret), auth_url, Some(token_url));

        Ok(Self { client })
    }

    pub fn authorize_url(&self) -> (Url, CsrfToken) {
        self.client.authorize_url(CsrfToken::new_random).url()
    }
}

#[async_trait]
impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = BackendError;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        if creds.old_state.secret() != creds.old_state.secret() {
            return Ok(None);
        }

        let token = self
            .client
            .exchange_code(AuthorizationCode::new(creds.code))
            .request_async(async_http_client)
            .await?;

        let github_info = crate::HTTP_CLIENT
            .get("https://api.github.com/user")
            .bearer_auth(token.access_token().secret())
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await?
            .error_for_status()?
            .json::<GitHubResponse>()
            .await?;

        let user = User {
            id: github_info.id,
            username: github_info.login,
            access_token: token.access_token().secret().clone(),
        };

        USERS.lock().insert(user.id, user.clone());

        Ok(Some(user))
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        Ok(USERS.lock().get(user_id).cloned())
    }
}
