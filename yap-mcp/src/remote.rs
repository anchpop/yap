//! Remote MCP server: streamable HTTP + OAuth 2.1, so yap can be added as a
//! claude.ai custom connector.
//!
//! The OAuth layer is a thin shim over Supabase auth: access tokens ARE
//! Supabase user JWTs (verified with the project JWT secret, exactly like
//! yap-ai-backend does), token refresh proxies to Supabase, and the authorize
//! page logs the user into their yap account with email + password. All event
//! reads/writes then happen as the user themself, inside RLS. The only
//! server-side OAuth state is the in-flight authorization codes; client
//! registrations are encoded as signed client_ids, so restarts don't break
//! existing connections.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use axum::{
    Form, Json, Router,
    extract::{Query, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use base64::Engine as _;
use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead as _, KeyInit as _},
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Validation};
use rand::Rng as _;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use tokio::sync::Mutex;

use crate::deck::PackCache;
use crate::server::{YapMcp, YapState};
use crate::sync::SupabaseAuth;

/// How long an authorization code stays exchangeable.
const CODE_TTL: Duration = Duration::from_secs(300);

/// The Supabase anon key — public by design (it ships in the web frontend).
const DEFAULT_ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImVlYXJ3enFvdHBmb2RlcnBmcnF4Iiwicm9sZSI6ImFub24iLCJpYXQiOjE3NDgyMTUwOTIsImV4cCI6MjA2Mzc5MTA5Mn0.BmnDrHtD-THaSLHO9VE2X-PO6B-z9OkbxzjeIinN6b8";

pub struct RemoteConfig {
    /// Public base URL of this server (no trailing slash), used in OAuth
    /// metadata and Host validation, e.g. `https://mcp.yap.town`.
    pub base_url: String,
    pub port: u16,
    pub supabase_url: String,
    pub supabase_anon_key: String,
    /// The Supabase project JWT secret, used to verify user access tokens and
    /// to sign client_ids.
    pub jwt_secret: String,
    pub out_dir: PathBuf,
}

impl RemoteConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let port: u16 = std::env::var("PORT")
            .ok()
            .map(|p| p.parse())
            .transpose()
            .context("PORT must be a number")?
            .unwrap_or(8080);
        let base_url = std::env::var("YAP_MCP_BASE_URL")
            .unwrap_or_else(|_| format!("http://localhost:{port}"))
            .trim_end_matches('/')
            .to_string();
        let supabase_url = std::env::var("SUPABASE_URL")
            .unwrap_or_else(|_| "https://eearwzqotpfoderpfrqx.supabase.co".to_string());
        let supabase_anon_key =
            std::env::var("SUPABASE_ANON_KEY").unwrap_or_else(|_| DEFAULT_ANON_KEY.to_string());
        let jwt_secret = std::env::var("SUPABASE_JWT_SECRET")
            .context("SUPABASE_JWT_SECRET env var required (verifies user tokens)")?;
        let out_dir = std::env::var("YAP_OUT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../out")));
        Ok(RemoteConfig {
            base_url,
            port,
            supabase_url,
            supabase_anon_key,
            jwt_secret,
            out_dir,
        })
    }
}

/// A user authenticated by the bearer token on the current request. Inserted
/// into request extensions by [`require_auth`]; tools read it back out of the
/// propagated request parts.
#[derive(Clone)]
pub struct AuthedUser {
    pub user_id: String,
    /// The raw Supabase access token, reused for Supabase REST calls.
    pub bearer: String,
}

/// An authorization code waiting to be exchanged at the token endpoint.
struct PendingCode {
    session: SupabaseSession,
    code_challenge: String,
    client_id: String,
    redirect_uri: String,
    created: Instant,
}

pub struct RemoteApp {
    pub config: RemoteConfig,
    http: reqwest::Client,
    packs: PackCache,
    /// Encrypts the Supabase session embedded in the tokens we mint, so the
    /// OAuth client never holds a raw Supabase credential.
    token_cipher: ChaCha20Poly1305,
    users: Mutex<HashMap<String, Arc<Mutex<YapState>>>>,
    codes: Mutex<HashMap<String, PendingCode>>,
}

impl RemoteApp {
    pub fn new(config: RemoteConfig) -> Self {
        let packs = PackCache::new(config.out_dir.clone());
        // Domain-separated key derivation from the project JWT secret.
        let key = Sha256::digest(format!("yap-mcp token encryption:{}", config.jwt_secret));
        let token_cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        RemoteApp {
            config,
            http: reqwest::Client::new(),
            packs,
            token_cipher,
            users: Mutex::new(HashMap::new()),
            codes: Mutex::new(HashMap::new()),
        }
    }

    fn encrypt(&self, plaintext: &str) -> String {
        let nonce_bytes: [u8; 12] = rand::rng().random();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .token_cipher
            .encrypt(nonce, plaintext.as_bytes())
            .expect("ChaCha20-Poly1305 encryption cannot fail");
        let mut blob = nonce_bytes.to_vec();
        blob.extend(ciphertext);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(blob)
    }

    fn decrypt(&self, blob: &str) -> Option<String> {
        let blob = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(blob)
            .ok()?;
        let (nonce_bytes, ciphertext) = blob.split_at_checked(12)?;
        let plaintext = self
            .token_cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .ok()?;
        String::from_utf8(plaintext).ok()
    }

    /// Get (or lazily initialize) the deck state for an authenticated user,
    /// refreshing the bearer token it uses for Supabase calls.
    pub async fn state_for_user(&self, user: &AuthedUser) -> anyhow::Result<Arc<Mutex<YapState>>> {
        if let Some(slot) = self.users.lock().await.get(&user.user_id).cloned() {
            slot.lock().await.set_bearer(user.bearer.clone());
            return Ok(slot);
        }

        // Initialize outside the map lock so one user's slow first load (event
        // fetch + possible language pack load) doesn't block everyone else. If
        // two requests race, the loser's state is simply dropped.
        let supabase = SupabaseAuth {
            url: self.config.supabase_url.clone(),
            apikey: self.config.supabase_anon_key.clone(),
            bearer: user.bearer.clone(),
        };
        let state = YapState::for_user(supabase, user.user_id.clone(), &self.packs).await?;
        let slot = Arc::new(Mutex::new(state));
        let mut users = self.users.lock().await;
        Ok(users.entry(user.user_id.clone()).or_insert(slot).clone())
    }
}

/// Run the remote server. Serves OAuth + metadata publicly and the MCP
/// endpoint behind bearer auth.
pub async fn serve(app: Arc<RemoteApp>) -> anyhow::Result<()> {
    let mcp_service: StreamableHttpService<YapMcp, LocalSessionManager> =
        StreamableHttpService::new(
            {
                let app = app.clone();
                move || Ok(YapMcp::new_remote(app.clone()))
            },
            Arc::new(LocalSessionManager::default()),
            {
                let mut config = StreamableHttpServerConfig::default();
                config.allowed_hosts = allowed_hosts(&app.config);
                config
            },
        );

    let mcp_router = Router::new().nest_service("/mcp", mcp_service).layer(
        axum::middleware::from_fn_with_state(app.clone(), require_auth),
    );

    let public = Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/mcp",
            get(authorization_server_metadata),
        )
        .route(
            "/.well-known/openid-configuration",
            get(authorization_server_metadata),
        )
        .route("/oauth/register", post(register))
        .route(
            "/oauth/authorize",
            get(authorize_page).post(authorize_submit),
        )
        .route("/oauth/token", post(token))
        .route("/health", get(|| async { "ok" }))
        .with_state(app.clone());

    let router = public
        .merge(mcp_router)
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr = format!("0.0.0.0:{}", app.config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    log::info!(
        "yap-mcp-server listening on {addr} ({})",
        app.config.base_url
    );
    axum::serve(listener, router).await?;
    Ok(())
}

/// Hosts we accept in the `Host` header (DNS-rebinding protection).
fn allowed_hosts(config: &RemoteConfig) -> Vec<String> {
    let mut hosts = vec![
        "localhost".to_string(),
        format!("localhost:{}", config.port),
        "127.0.0.1".to_string(),
        format!("127.0.0.1:{}", config.port),
    ];
    if let Ok(url) = reqwest::Url::parse(&config.base_url)
        && let Some(host) = url.host_str()
    {
        hosts.push(host.to_string());
        if let Some(port) = url.port() {
            hosts.push(format!("{host}:{port}"));
        }
    }
    hosts
}

/// Audience values for the tokens we mint. Scoping tokens to this server means
/// the OAuth client (claude.ai) never holds a credential usable against
/// Supabase directly — only against /mcp here.
const ACCESS_AUD: &str = "yap-mcp";
const REFRESH_AUD: &str = "yap-mcp-refresh";

#[derive(Serialize, Deserialize)]
struct AccessClaims {
    iss: String,
    aud: String,
    sub: String,
    exp: u64,
    /// The user's Supabase access token, encrypted with the server's key.
    sb: String,
}

#[derive(Serialize, Deserialize)]
struct RefreshClaims {
    iss: String,
    aud: String,
    sub: String,
    /// The user's Supabase refresh token, encrypted with the server's key.
    sbr: String,
}

/// Mint the OAuth token response for a fresh Supabase session.
fn issue_tokens(app: &RemoteApp, session: &SupabaseSession) -> serde_json::Value {
    let key = EncodingKey::from_secret(app.config.jwt_secret.as_bytes());
    let access = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &AccessClaims {
            iss: app.config.base_url.clone(),
            aud: ACCESS_AUD.to_string(),
            sub: session.user.id.clone(),
            exp: chrono::Utc::now().timestamp() as u64 + session.expires_in,
            sb: app.encrypt(&session.access_token),
        },
        &key,
    )
    .expect("HS256 signing cannot fail");
    let refresh = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &RefreshClaims {
            iss: app.config.base_url.clone(),
            aud: REFRESH_AUD.to_string(),
            sub: session.user.id.clone(),
            sbr: app.encrypt(&session.refresh_token),
        },
        &key,
    )
    .expect("HS256 signing cannot fail");
    json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": session.expires_in,
        "refresh_token": refresh,
        "scope": "yap",
    })
}

/// Verify one of our access tokens and recover the user + their Supabase token.
fn verify_access_token(app: &RemoteApp, token: &str) -> Option<AuthedUser> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[ACCESS_AUD]);
    let claims = jsonwebtoken::decode::<AccessClaims>(
        token,
        &DecodingKey::from_secret(app.config.jwt_secret.as_bytes()),
        &validation,
    )
    .ok()?
    .claims;
    Some(AuthedUser {
        user_id: claims.sub,
        bearer: app.decrypt(&claims.sb)?,
    })
}

/// Verify one of our refresh tokens and recover the Supabase refresh token.
fn verify_refresh_token(app: &RemoteApp, token: &str) -> Option<String> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[REFRESH_AUD]);
    validation.validate_exp = false;
    validation.required_spec_claims.clear();
    let claims = jsonwebtoken::decode::<RefreshClaims>(
        token,
        &DecodingKey::from_secret(app.config.jwt_secret.as_bytes()),
        &validation,
    )
    .ok()?
    .claims;
    app.decrypt(&claims.sbr)
}

async fn require_auth(
    State(app): State<Arc<RemoteApp>>,
    mut req: axum::extract::Request,
    next: Next,
) -> Response {
    let user = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| verify_access_token(&app, token));
    match user {
        Some(user) => {
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                format!(
                    r#"Bearer resource_metadata="{}/.well-known/oauth-protected-resource""#,
                    app.config.base_url
                ),
            )],
            "unauthorized",
        )
            .into_response(),
    }
}

async fn protected_resource_metadata(State(app): State<Arc<RemoteApp>>) -> Json<serde_json::Value> {
    let base = &app.config.base_url;
    Json(json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base],
        "bearer_methods_supported": ["header"],
    }))
}

async fn authorization_server_metadata(
    State(app): State<Arc<RemoteApp>>,
) -> Json<serde_json::Value> {
    let base = &app.config.base_url;
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": ["yap"],
    }))
}

/// Client registrations are stateless: the client_id is a signed JWT carrying
/// the registered redirect URIs, validated at authorize/token time.
#[derive(Serialize, Deserialize)]
struct ClientClaims {
    redirect_uris: Vec<String>,
}

fn sign_client_id(jwt_secret: &str, redirect_uris: Vec<String>) -> String {
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &ClientClaims { redirect_uris },
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .expect("HS256 signing cannot fail")
}

fn verify_client_id(jwt_secret: &str, client_id: &str) -> Option<Vec<String>> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false;
    validation.required_spec_claims.clear();
    jsonwebtoken::decode::<ClientClaims>(
        client_id,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &validation,
    )
    .ok()
    .map(|data| data.claims.redirect_uris)
}

#[derive(Deserialize)]
struct RegistrationRequest {
    redirect_uris: Vec<String>,
    #[serde(default)]
    client_name: Option<String>,
}

async fn register(
    State(app): State<Arc<RemoteApp>>,
    Json(req): Json<RegistrationRequest>,
) -> Response {
    if req.redirect_uris.is_empty() {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            "redirect_uris must not be empty",
        );
    }
    for uri in &req.redirect_uris {
        let ok = reqwest::Url::parse(uri).is_ok_and(|u| {
            u.scheme() == "https"
                || (u.scheme() == "http"
                    && matches!(u.host_str(), Some("localhost") | Some("127.0.0.1")))
        });
        if !ok {
            return oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_redirect_uri",
                &format!("invalid redirect_uri: {uri}"),
            );
        }
    }
    let client_id = sign_client_id(&app.config.jwt_secret, req.redirect_uris.clone());
    (
        StatusCode::CREATED,
        Json(json!({
            "client_id": client_id,
            "redirect_uris": req.redirect_uris,
            "token_endpoint_auth_method": "none",
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "client_name": req.client_name,
            "client_id_issued_at": chrono::Utc::now().timestamp(),
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct AuthorizeParams {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    #[serde(default)]
    state: Option<String>,
    code_challenge: String,
    #[serde(default)]
    code_challenge_method: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

fn validate_authorize(app: &RemoteApp, params: &AuthorizeParams) -> Result<(), String> {
    if params.response_type != "code" {
        return Err("response_type must be 'code'".to_string());
    }
    let redirect_uris = verify_client_id(&app.config.jwt_secret, &params.client_id)
        .ok_or("unknown client_id — register via the registration endpoint first")?;
    if !redirect_uris.contains(&params.redirect_uri) {
        return Err("redirect_uri is not registered for this client".to_string());
    }
    if params.code_challenge.is_empty() {
        return Err("code_challenge (PKCE) is required".to_string());
    }
    match params.code_challenge_method.as_deref() {
        Some("S256") | None => Ok(()),
        Some(other) => Err(format!("unsupported code_challenge_method '{other}'")),
    }
}

async fn authorize_page(
    State(app): State<Arc<RemoteApp>>,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    if let Err(e) = validate_authorize(&app, &params) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    Html(login_page(&params, None)).into_response()
}

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
    response_type: String,
    client_id: String,
    redirect_uri: String,
    #[serde(default)]
    state: Option<String>,
    code_challenge: String,
    #[serde(default)]
    code_challenge_method: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

impl LoginForm {
    fn params(&self) -> AuthorizeParams {
        AuthorizeParams {
            response_type: self.response_type.clone(),
            client_id: self.client_id.clone(),
            redirect_uri: self.redirect_uri.clone(),
            state: self.state.clone(),
            code_challenge: self.code_challenge.clone(),
            code_challenge_method: self.code_challenge_method.clone(),
            scope: self.scope.clone(),
        }
    }
}

#[derive(Deserialize)]
struct SupabaseSession {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
    user: SupabaseUser,
}

#[derive(Deserialize)]
struct SupabaseUser {
    id: String,
}

async fn authorize_submit(
    State(app): State<Arc<RemoteApp>>,
    Form(form): Form<LoginForm>,
) -> Response {
    let params = form.params();
    if let Err(e) = validate_authorize(&app, &params) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let resp = app
        .http
        .post(format!(
            "{}/auth/v1/token?grant_type=password",
            app.config.supabase_url
        ))
        .header("apikey", &app.config.supabase_anon_key)
        .json(&json!({ "email": form.email, "password": form.password }))
        .send()
        .await;
    let session: SupabaseSession = match resp {
        Ok(resp) if resp.status().is_success() => match resp.json().await {
            Ok(session) => session,
            Err(e) => {
                log::error!("failed to parse Supabase session: {e}");
                return Html(login_page(
                    &params,
                    Some("Something went wrong — try again."),
                ))
                .into_response();
            }
        },
        Ok(_) => {
            return Html(login_page(&params, Some("Wrong email or password."))).into_response();
        }
        Err(e) => {
            log::error!("Supabase login request failed: {e}");
            return Html(login_page(
                &params,
                Some("Something went wrong — try again."),
            ))
            .into_response();
        }
    };

    let code = {
        let bytes: [u8; 32] = rand::rng().random();
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    };
    {
        let mut codes = app.codes.lock().await;
        codes.retain(|_, pending| pending.created.elapsed() < CODE_TTL);
        codes.insert(
            code.clone(),
            PendingCode {
                session,
                code_challenge: params.code_challenge.clone(),
                client_id: params.client_id.clone(),
                redirect_uri: params.redirect_uri.clone(),
                created: Instant::now(),
            },
        );
    }

    let mut url = match reqwest::Url::parse(&params.redirect_uri) {
        Ok(url) => url,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid redirect_uri").into_response(),
    };
    url.query_pairs_mut().append_pair("code", &code);
    if let Some(state) = &params.state {
        url.query_pairs_mut().append_pair("state", state);
    }
    Redirect::to(url.as_str()).into_response()
}

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    redirect_uri: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    code_verifier: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
}

async fn token(State(app): State<Arc<RemoteApp>>, Form(req): Form<TokenRequest>) -> Response {
    match req.grant_type.as_str() {
        "authorization_code" => {
            let Some(code) = &req.code else {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_request", "missing code");
            };
            // Single use: the code is removed even if the checks below fail.
            let Some(pending) = app.codes.lock().await.remove(code) else {
                return oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "unknown or already-used code",
                );
            };
            if pending.created.elapsed() > CODE_TTL {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", "code expired");
            }
            if req.redirect_uri.as_deref() != Some(pending.redirect_uri.as_str()) {
                return oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "redirect_uri mismatch",
                );
            }
            if req.client_id.as_deref() != Some(pending.client_id.as_str()) {
                return oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "client_id mismatch",
                );
            }
            let verifier_ok = req.code_verifier.as_deref().is_some_and(|verifier| {
                let digest = Sha256::digest(verifier.as_bytes());
                let computed = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
                computed == pending.code_challenge
            });
            if !verifier_ok {
                return oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "PKCE verification failed",
                );
            }
            Json(issue_tokens(&app, &pending.session)).into_response()
        }
        "refresh_token" => {
            let Some(refresh_token) = &req.refresh_token else {
                return oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "missing refresh_token",
                );
            };
            let Some(supabase_refresh_token) = verify_refresh_token(&app, refresh_token) else {
                return oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "refresh token was rejected",
                );
            };
            let resp = app
                .http
                .post(format!(
                    "{}/auth/v1/token?grant_type=refresh_token",
                    app.config.supabase_url
                ))
                .header("apikey", &app.config.supabase_anon_key)
                .json(&json!({ "refresh_token": supabase_refresh_token }))
                .send()
                .await;
            match resp {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<SupabaseSession>().await {
                        Ok(session) => Json(issue_tokens(&app, &session)).into_response(),
                        Err(e) => {
                            log::error!("failed to parse Supabase refresh response: {e}");
                            oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", "refresh failed")
                        }
                    }
                }
                _ => oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "refresh token was rejected",
                ),
            }
        }
        other => oauth_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            &format!("unsupported grant_type '{other}'"),
        ),
    }
}

fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    (
        status,
        Json(json!({ "error": error, "error_description": description })),
    )
        .into_response()
}

fn login_page(params: &AuthorizeParams, error: Option<&str>) -> String {
    let attr = |v: &str| html_escape::encode_double_quoted_attribute(v).into_owned();
    let hidden = |name: &str, value: &str| {
        format!(
            r#"<input type="hidden" name="{name}" value="{}">"#,
            attr(value)
        )
    };
    let mut hidden_fields = String::new();
    hidden_fields.push_str(&hidden("response_type", &params.response_type));
    hidden_fields.push_str(&hidden("client_id", &params.client_id));
    hidden_fields.push_str(&hidden("redirect_uri", &params.redirect_uri));
    if let Some(state) = &params.state {
        hidden_fields.push_str(&hidden("state", state));
    }
    hidden_fields.push_str(&hidden("code_challenge", &params.code_challenge));
    if let Some(method) = &params.code_challenge_method {
        hidden_fields.push_str(&hidden("code_challenge_method", method));
    }
    if let Some(scope) = &params.scope {
        hidden_fields.push_str(&hidden("scope", scope));
    }
    let error_html = error
        .map(|e| format!(r#"<p class="error">{}</p>"#, html_escape::encode_text(e)))
        .unwrap_or_default();

    format!(
        r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Connect to yap.town</title>
<style>
  body {{ font-family: system-ui, sans-serif; background: #faf7f0; display: flex;
         justify-content: center; align-items: center; min-height: 100vh; margin: 0; }}
  .card {{ background: white; border-radius: 12px; padding: 2rem; width: 320px;
           box-shadow: 0 4px 24px rgba(0,0,0,0.08); }}
  h1 {{ font-size: 1.2rem; margin: 0 0 0.5rem; }}
  p {{ color: #555; font-size: 0.9rem; margin: 0 0 1.25rem; }}
  label {{ display: block; font-size: 0.8rem; color: #333; margin-bottom: 0.25rem; }}
  input[type=email], input[type=password] {{ width: 100%; box-sizing: border-box;
    padding: 0.5rem; margin-bottom: 1rem; border: 1px solid #ddd; border-radius: 6px; }}
  button {{ width: 100%; padding: 0.6rem; background: #1a1a1a; color: white;
    border: none; border-radius: 6px; font-size: 0.95rem; cursor: pointer; }}
  .error {{ color: #c0392b; }}
</style>
</head>
<body>
  <div class="card">
    <h1>Connect to yap.town</h1>
    <p>Sign in to let this app do reviews, add words, and read your stats.</p>
    {error_html}
    <form method="post" action="/oauth/authorize">
      {hidden_fields}
      <label for="email">Email</label>
      <input type="email" id="email" name="email" required autofocus>
      <label for="password">Password</label>
      <input type="password" id="password" name="password" required>
      <button type="submit">Sign in &amp; connect</button>
    </form>
  </div>
</body>
</html>"#
    )
}
