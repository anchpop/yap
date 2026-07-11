//! Remote MCP server: streamable HTTP + OAuth 2.1, so yap can be added as a
//! claude.ai custom connector.
//!
//! The OAuth layer is a thin shim over Supabase auth: access tokens ARE
//! Supabase user JWTs (verified with the project JWT secret, exactly like
//! yap-ai-backend does), token refresh proxies to Supabase, and authorization
//! happens on the yap.town frontend: /oauth/authorize parks the request and
//! redirects to yap.town/connect, where the user signs in with their normal
//! yap auth (passkey-ready, same domain) and approves; the frontend then
//! posts their proof back to /oauth/approve and we mint the connector a
//! fresh Supabase session of its own. All event
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
    response::{IntoResponse, Redirect, Response},
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
    /// Service role key, used only to mint a fresh Supabase session for the
    /// connector when the user approves on the frontend.
    pub service_role_key: String,
    /// The yap.town frontend origin hosting the /connect approval page.
    pub frontend_url: String,
    /// Extra hostnames to accept in the Host header (e.g. the fly.dev name
    /// when the public domain is proxied in front of it).
    pub extra_allowed_hosts: Vec<String>,
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
        let service_role_key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
            .context("SUPABASE_SERVICE_ROLE_KEY env var required (mints connector sessions)")?;
        let frontend_url = std::env::var("YAP_FRONTEND_URL")
            .unwrap_or_else(|_| "https://yap.town".to_string())
            .trim_end_matches('/')
            .to_string();
        let extra_allowed_hosts = std::env::var("YAP_MCP_ALLOWED_HOSTS")
            .map(|hosts| {
                hosts
                    .split(',')
                    .map(|host| host.trim().to_string())
                    .filter(|host| !host.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let out_dir = std::env::var("YAP_OUT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../out")));
        Ok(RemoteConfig {
            base_url,
            port,
            supabase_url,
            supabase_anon_key,
            jwt_secret,
            service_role_key,
            frontend_url,
            extra_allowed_hosts,
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

/// An authorize request parked while the user signs in and approves it on the
/// yap.town frontend.
struct PendingAuth {
    client_id: String,
    redirect_uri: String,
    state: Option<String>,
    code_challenge: String,
    created: Instant,
}

/// A user's cached deck state plus when it was last touched, so idle users
/// can be evicted (the state is cheap to rebuild from events on their next
/// request; the heavyweight language packs are shared and stay cached).
struct UserSlot {
    state: Arc<Mutex<YapState>>,
    last_used: Instant,
}

/// Evict a user's cached state after this much inactivity.
const USER_STATE_IDLE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// One fixed rate-limit window on a public endpoint, keyed by (endpoint, ip).
struct RateWindow {
    started: Instant,
    count: u32,
}

const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

pub struct RemoteApp {
    pub config: RemoteConfig,
    http: reqwest::Client,
    packs: PackCache,
    /// Encrypts the Supabase session embedded in the tokens we mint, so the
    /// OAuth client never holds a raw Supabase credential.
    token_cipher: ChaCha20Poly1305,
    /// The built review widget (None when not built).
    pub widget: Option<Arc<crate::server::Widget>>,
    users: Mutex<HashMap<String, UserSlot>>,
    codes: Mutex<HashMap<String, PendingCode>>,
    pending_auth: Mutex<HashMap<String, PendingAuth>>,
    rate_limits: Mutex<HashMap<(&'static str, String), RateWindow>>,
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
            widget: crate::server::load_widget_html(),
            users: Mutex::new(HashMap::new()),
            codes: Mutex::new(HashMap::new()),
            pending_auth: Mutex::new(HashMap::new()),
            rate_limits: Mutex::new(HashMap::new()),
        }
    }

    /// Fixed-window per-IP rate limit. Returns false when the caller should
    /// get a 429. Best-effort: the IP comes from proxy headers, so this stops
    /// floods and accidents, not a determined attacker with many addresses.
    async fn allow_rate(&self, endpoint: &'static str, ip: String, limit: u32) -> bool {
        let mut limits = self.rate_limits.lock().await;
        limits.retain(|_, w| w.started.elapsed() < RATE_LIMIT_WINDOW);
        let window = limits.entry((endpoint, ip)).or_insert(RateWindow {
            started: Instant::now(),
            count: 0,
        });
        window.count += 1;
        window.count <= limit
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
        {
            let mut users = self.users.lock().await;
            users.retain(|_, slot| slot.last_used.elapsed() < USER_STATE_IDLE_TTL);
            if let Some(slot) = users.get_mut(&user.user_id) {
                slot.last_used = Instant::now();
                let state = slot.state.clone();
                drop(users);
                state.lock().await.set_bearer(user.bearer.clone());
                return Ok(state);
            }
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
        Ok(users
            .entry(user.user_id.clone())
            .or_insert(UserSlot {
                state: slot,
                last_used: Instant::now(),
            })
            .state
            .clone())
    }
}

/// Best-effort client IP for rate limiting: Cloudflare's header when we're
/// behind the mcp.yap.town proxy, Fly's when hit directly, else unknown (one
/// shared bucket — still bounds total damage).
fn client_ip(headers: &axum::http::HeaderMap) -> String {
    for header in ["cf-connecting-ip", "fly-client-ip"] {
        if let Some(ip) = headers.get(header).and_then(|v| v.to_str().ok()) {
            return ip.to_string();
        }
    }
    "unknown".to_string()
}

fn rate_limited() -> Response {
    oauth_error(
        StatusCode::TOO_MANY_REQUESTS,
        "slow_down",
        "too many requests from your address — wait a minute and try again",
    )
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
                // Stateless: no server-side session state, so a deploy that
                // replaces the machine can't strand a client on a session id
                // the new process never issued (the old failure surfaced by
                // the Anthropic proxy as an opaque "Invalid content from
                // server"). Auth is a per-request bearer anyway, so every
                // request is already self-contained. `json_response` returns
                // plain application/json instead of SSE framing.
                config.stateful_mode = false;
                config.json_response = true;
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
        .route("/oauth/authorize", get(authorize_page))
        .route("/oauth/approve", post(approve))
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
    hosts.extend(config.extra_allowed_hosts.iter().cloned());
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
    headers: axum::http::HeaderMap,
    Json(req): Json<RegistrationRequest>,
) -> Response {
    if !app.allow_rate("register", client_ip(&headers), 30).await {
        return rate_limited();
    }
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

/// Start the flow: validate the client's request, park it, and send the user
/// to the yap.town frontend to sign in and approve (auth lives on the main
/// domain so passkeys can work there). The frontend posts back to
/// /oauth/approve.
async fn authorize_page(
    State(app): State<Arc<RemoteApp>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    if !app.allow_rate("authorize", client_ip(&headers), 60).await {
        return rate_limited();
    }
    if let Err(e) = validate_authorize(&app, &params) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let request_id = random_token();
    {
        let mut pending = app.pending_auth.lock().await;
        pending.retain(|_, p| p.created.elapsed() < CODE_TTL);
        pending.insert(
            request_id.clone(),
            PendingAuth {
                client_id: params.client_id.clone(),
                redirect_uri: params.redirect_uri.clone(),
                state: params.state.clone(),
                code_challenge: params.code_challenge.clone(),
                created: Instant::now(),
            },
        );
    }

    let mut url = match reqwest::Url::parse(&format!("{}/connect", app.config.frontend_url)) {
        Ok(url) => url,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "bad frontend url").into_response();
        }
    };
    url.query_pairs_mut()
        .append_pair("request", &request_id)
        // Which whitelisted MCP origin the frontend should post approval to
        // (lets the same page work against a locally-run server in dev).
        .append_pair("mcp", &app.config.base_url);
    Redirect::to(url.as_str()).into_response()
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

/// Claims of a Supabase user access token (as opposed to the tokens we mint).
#[derive(Deserialize)]
struct SupabaseAccessClaims {
    sub: String,
    email: String,
}

fn verify_supabase_token(app: &RemoteApp, token: &str) -> Option<SupabaseAccessClaims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["authenticated"]);
    jsonwebtoken::decode::<SupabaseAccessClaims>(
        token,
        &DecodingKey::from_secret(app.config.jwt_secret.as_bytes()),
        &validation,
    )
    .ok()
    .map(|data| data.claims)
}

/// Mint a fresh Supabase session for the connector via the admin
/// generate_link + verify dance (the same trick the impersonation tool uses).
/// The browser's own session must not be reused: two devices sharing one
/// refresh-token rotation family would invalidate each other.
async fn mint_session(app: &RemoteApp, email: &str) -> anyhow::Result<SupabaseSession> {
    let service_key = &app.config.service_role_key;
    let link: serde_json::Value = app
        .http
        .post(format!(
            "{}/auth/v1/admin/generate_link",
            app.config.supabase_url
        ))
        .header("apikey", service_key)
        .header("Authorization", format!("Bearer {service_key}"))
        .json(&json!({ "type": "magiclink", "email": email }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let token_hash = link["hashed_token"]
        .as_str()
        .or_else(|| link["properties"]["hashed_token"].as_str())
        .context("generate_link response missing hashed_token")?;

    let session: SupabaseSession = app
        .http
        .post(format!("{}/auth/v1/verify", app.config.supabase_url))
        .header("apikey", &app.config.supabase_anon_key)
        .json(&json!({ "type": "magiclink", "token_hash": token_hash }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(session)
}

#[derive(Deserialize)]
struct ApproveRequest {
    request_id: String,
    /// The signed-in user's Supabase access token, proving who is approving.
    access_token: String,
}

/// Called by the yap.town /connect page when the user approves. Exchanges
/// their proof of identity for an authorization code and tells the frontend
/// where to send the user next.
async fn approve(
    State(app): State<Arc<RemoteApp>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ApproveRequest>,
) -> Response {
    if !app.allow_rate("approve", client_ip(&headers), 30).await {
        return rate_limited();
    }
    let Some(pending) = app.pending_auth.lock().await.remove(&req.request_id) else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "unknown or expired connect request — start over from your MCP client",
        );
    };
    if pending.created.elapsed() > CODE_TTL {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "connect request expired — start over from your MCP client",
        );
    }
    let Some(claims) = verify_supabase_token(&app, &req.access_token) else {
        return oauth_error(
            StatusCode::UNAUTHORIZED,
            "access_denied",
            "your yap session is invalid or expired — sign in again",
        );
    };

    let session = match mint_session(&app, &claims.email).await {
        Ok(session) => session,
        Err(e) => {
            log::error!("failed to mint connector session for {}: {e:#}", claims.sub);
            return oauth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "could not create a session — try again",
            );
        }
    };
    if session.user.id != claims.sub {
        log::error!(
            "minted session user {} does not match approver {}",
            session.user.id,
            claims.sub
        );
        return oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "could not create a session — try again",
        );
    }

    let code = random_token();
    {
        let mut codes = app.codes.lock().await;
        codes.retain(|_, pending| pending.created.elapsed() < CODE_TTL);
        codes.insert(
            code.clone(),
            PendingCode {
                session,
                code_challenge: pending.code_challenge,
                client_id: pending.client_id,
                redirect_uri: pending.redirect_uri.clone(),
                created: Instant::now(),
            },
        );
    }

    let mut url = match reqwest::Url::parse(&pending.redirect_uri) {
        Ok(url) => url,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid redirect_uri").into_response(),
    };
    url.query_pairs_mut().append_pair("code", &code);
    if let Some(state) = &pending.state {
        url.query_pairs_mut().append_pair("state", state);
    }
    Json(json!({ "redirect": url.as_str() })).into_response()
}

fn random_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
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

async fn token(
    State(app): State<Arc<RemoteApp>>,
    headers: axum::http::HeaderMap,
    Form(req): Form<TokenRequest>,
) -> Response {
    if !app.allow_rate("token", client_ip(&headers), 120).await {
        return rate_limited();
    }
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
