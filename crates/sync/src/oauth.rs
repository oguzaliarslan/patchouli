//! Browser login.
//!
//! The app opens the service's authorization page in the browser, and the
//! service redirects back to a server on `localhost` that this module runs.
//!
//! - **AniList**: implicit grant. The token arrives in the URL fragment, which
//!   browsers don't send to servers, so the first response is a tiny page
//!   that moves the fragment into the query string and reloads.
//! - **MyAnimeList**: authorization code with PKCE. MyAnimeList only supports
//!   the `plain` challenge method, so the challenge is the verifier itself.
//!
//! Each service must have an app registered with the redirect URL
//! `http://localhost:<port>/callback/<service>` (see [`OAuthConfig`]).

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use patchouli_core::ServiceId;
use reqwest::Url;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::credentials::Credentials;
use crate::{Error, Result, anilist, http, myanimelist};

pub const DEFAULT_REDIRECT_PORT: u16 = 47312;

/// How long to wait for the user to finish logging in.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Client IDs and the redirect port.
///
/// Values come from environment variables at run time, or failing that at
/// build time: `TRACKER_ANILIST_CLIENT_ID`, `TRACKER_MAL_CLIENT_ID` and
/// `TRACKER_OAUTH_PORT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthConfig {
    pub anilist_client_id: Option<String>,
    pub myanimelist_client_id: Option<String>,
    pub redirect_port: u16,
}

impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            anilist_client_id: None,
            myanimelist_client_id: None,
            redirect_port: DEFAULT_REDIRECT_PORT,
        }
    }
}

impl OAuthConfig {
    pub fn from_env() -> Self {
        fn var(name: &str, built: Option<&'static str>) -> Option<String> {
            std::env::var(name)
                .ok()
                .or(built.map(str::to_owned))
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        }
        Self {
            anilist_client_id: var(
                "TRACKER_ANILIST_CLIENT_ID",
                option_env!("TRACKER_ANILIST_CLIENT_ID"),
            ),
            myanimelist_client_id: var(
                "TRACKER_MAL_CLIENT_ID",
                option_env!("TRACKER_MAL_CLIENT_ID"),
            ),
            redirect_port: var("TRACKER_OAUTH_PORT", option_env!("TRACKER_OAUTH_PORT"))
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_REDIRECT_PORT),
        }
    }

    pub fn client_id(&self, service: ServiceId) -> Result<&str> {
        let (id, variable) = match service {
            ServiceId::AniList => (&self.anilist_client_id, "TRACKER_ANILIST_CLIENT_ID"),
            ServiceId::MyAnimeList => (&self.myanimelist_client_id, "TRACKER_MAL_CLIENT_ID"),
            ServiceId::Kitsu => {
                return Err(Error::NotConfigured("Kitsu is not supported yet.".into()));
            }
        };
        id.as_deref().ok_or_else(|| {
            Error::NotConfigured(format!(
                "No {} client ID is configured. Set {variable} (see README).",
                service.name()
            ))
        })
    }

    pub fn is_configured(&self, service: ServiceId) -> bool {
        self.client_id(service).is_ok()
    }

    pub fn redirect_path(service: ServiceId) -> String {
        format!("/callback/{}", service.slug())
    }

    pub fn redirect_uri(&self, service: ServiceId) -> String {
        format!(
            "http://localhost:{}{}",
            self.redirect_port,
            Self::redirect_path(service)
        )
    }
}

/// A login waiting for the browser to come back.
pub struct PendingLogin {
    service: ServiceId,
    authorize_url: String,
    state: String,
    code_verifier: String,
    redirect_uri: String,
    client_id: String,
    server: LoopbackServer,
}

impl PendingLogin {
    /// Starts the redirect server and builds the authorization URL.
    pub async fn start(config: &OAuthConfig, service: ServiceId) -> Result<Self> {
        let client_id = config.client_id(service)?.to_owned();
        let server = LoopbackServer::bind(config.redirect_port).await?;
        let state = random_string(32)?;
        let code_verifier = random_string(64)?;
        let redirect_uri = config.redirect_uri(service);

        let authorize_url = match service {
            ServiceId::AniList => Url::parse_with_params(
                anilist::AUTHORIZE_URL,
                &[
                    ("client_id", client_id.as_str()),
                    ("response_type", "token"),
                ],
            ),
            _ => Url::parse_with_params(
                myanimelist::AUTHORIZE_URL,
                &[
                    ("response_type", "code"),
                    ("client_id", client_id.as_str()),
                    ("redirect_uri", redirect_uri.as_str()),
                    ("code_challenge", code_verifier.as_str()),
                    ("code_challenge_method", "plain"),
                    ("state", state.as_str()),
                ],
            ),
        }
        .map_err(|e| Error::Auth(e.to_string()))?
        .to_string();

        Ok(Self {
            service,
            authorize_url,
            state,
            code_verifier,
            redirect_uri,
            client_id,
            server,
        })
    }

    /// The page to open in the browser.
    pub fn authorize_url(&self) -> &str {
        &self.authorize_url
    }

    /// Waits for the redirect (up to [`LOGIN_TIMEOUT`]) and returns the tokens.
    pub async fn finish(self) -> Result<Credentials> {
        self.finish_with(LOGIN_TIMEOUT, myanimelist::TOKEN_URL)
            .await
    }

    pub(crate) async fn finish_with(
        self,
        timeout: Duration,
        token_url: &str,
    ) -> Result<Credentials> {
        let path = OAuthConfig::redirect_path(self.service);
        let forward_fragment = self.service == ServiceId::AniList;
        let params = tokio::time::timeout(timeout, self.server.wait(&path, forward_fragment))
            .await
            .map_err(|_| Error::Auth("Login timed out.".into()))??;

        if let Some(error) = params.get("error") {
            let description = params.get("error_description").unwrap_or(error);
            return Err(Error::Auth(format!("Login failed: {description}")));
        }

        match self.service {
            ServiceId::AniList => {
                let token = params
                    .get("access_token")
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| Error::Auth("AniList didn't return a token.".into()))?;
                Ok(Credentials {
                    access_token: token.clone(),
                    refresh_token: None,
                    expires_at: params
                        .get("expires_in")
                        .and_then(|s| s.parse::<i64>().ok())
                        .map(|s| crate::now() + s),
                })
            }
            _ => {
                if params.get("state") != Some(&self.state) {
                    return Err(Error::Auth(
                        "Login response didn't match the request.".into(),
                    ));
                }
                let code = params
                    .get("code")
                    .ok_or_else(|| Error::Auth("MyAnimeList didn't return a code.".into()))?;
                exchange_code(
                    &http::client(),
                    token_url,
                    &self.client_id,
                    code,
                    &self.code_verifier,
                    &self.redirect_uri,
                )
                .await
            }
        }
    }
}

/// Exchanges a MyAnimeList authorization code for tokens.
pub async fn exchange_code(
    http: &reqwest::Client,
    token_url: &str,
    client_id: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<Credentials> {
    myanimelist::request_token(
        http,
        token_url,
        &[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ],
    )
    .await
    .map_err(|e| Error::Auth(format!("Couldn't complete login: {e}")))
}

/// Random characters from the PKCE "unreserved" set.
fn random_string(len: usize) -> Result<String> {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).map_err(|e| Error::Auth(format!("No randomness: {e}")))?;
    Ok(bytes
        .into_iter()
        .map(|b| CHARS[usize::from(b & 63)] as char)
        .collect())
}

/// Minimal HTTP server on localhost (IPv4, and IPv6 when available) that waits
/// for the OAuth redirect.
pub struct LoopbackServer {
    v4: TcpListener,
    v6: Option<TcpListener>,
}

impl LoopbackServer {
    pub async fn bind(port: u16) -> Result<Self> {
        let v4 = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
            .await
            .map_err(|e| {
                Error::Auth(format!(
                    "Couldn't listen on port {port} for the login redirect: {e}"
                ))
            })?;
        let port = v4.local_addr()?.port();
        let v6 = TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port)))
            .await
            .ok();
        Ok(Self { v4, v6 })
    }

    pub fn port(&self) -> u16 {
        self.v4.local_addr().map(|a| a.port()).unwrap_or_default()
    }

    /// Serves requests until one arrives at `path` with a query string, and
    /// returns its parameters. With `forward_fragment`, a request at `path`
    /// without a query gets a page that resends the fragment as the query.
    pub async fn wait(
        &self,
        path: &str,
        forward_fragment: bool,
    ) -> Result<HashMap<String, String>> {
        loop {
            let (stream, _) = tokio::select! {
                accepted = self.v4.accept() => accepted?,
                accepted = accept(self.v6.as_ref()) => accepted?,
            };
            if let Some(params) = handle(stream, path, forward_fragment).await? {
                return Ok(params);
            }
        }
    }
}

async fn accept(listener: Option<&TcpListener>) -> std::io::Result<(TcpStream, SocketAddr)> {
    match listener {
        Some(listener) => listener.accept().await,
        None => std::future::pending().await,
    }
}

const FORWARD_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Logging in…</title>\
<script>location.replace(location.pathname + '?' + (location.hash.slice(1) || 'error=access_denied'));</script>\
<p>Logging in…</p>";

fn done_page(message: &str) -> String {
    format!(
        "<!doctype html><meta charset=utf-8><title>Patchouli</title>\
         <body style=\"font-family:system-ui;margin:3rem\"><p>{message}</p></body>"
    )
}

async fn handle(
    mut stream: TcpStream,
    path: &str,
    forward_fragment: bool,
) -> Result<Option<HashMap<String, String>>> {
    let Some(target) = read_request_target(&mut stream).await? else {
        respond(&mut stream, "400 Bad Request", "").await?;
        return Ok(None);
    };
    let Ok(url) = Url::parse(&format!("http://localhost{target}")) else {
        respond(&mut stream, "400 Bad Request", "").await?;
        return Ok(None);
    };
    if url.path() != path {
        respond(&mut stream, "404 Not Found", "").await?;
        return Ok(None);
    }
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    if params.is_empty() && forward_fragment {
        respond(&mut stream, "200 OK", FORWARD_PAGE).await?;
        return Ok(None);
    }
    let message = if params.contains_key("error") {
        "Login was cancelled or failed. You can close this tab and try again from the app."
    } else {
        "You're logged in. You can close this tab and return to the app."
    };
    respond(&mut stream, "200 OK", &done_page(message)).await?;
    Ok(Some(params))
}

/// Reads the request head and returns the target of a `GET` request.
async fn read_request_target(stream: &mut TcpStream) -> Result<Option<String>> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).await?;
        if read == 0 || buffer.len() > 16 * 1024 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let head = String::from_utf8_lossy(&buffer);
    let mut parts = head.lines().next().unwrap_or_default().split_whitespace();
    match (parts.next(), parts.next()) {
        (Some("GET"), Some(target)) if target.starts_with('/') => Ok(Some(target.to_owned())),
        _ => Ok(None),
    }
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await.ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(port: u16) -> OAuthConfig {
        OAuthConfig {
            anilist_client_id: Some("123".into()),
            myanimelist_client_id: Some("abc".into()),
            redirect_port: port,
        }
    }

    async fn get(port: u16, target: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[test]
    fn missing_client_id_is_reported() {
        let error = OAuthConfig::default()
            .client_id(ServiceId::AniList)
            .unwrap_err();
        assert!(error.to_string().contains("TRACKER_ANILIST_CLIENT_ID"));
    }

    #[test]
    fn random_strings_use_unreserved_characters() {
        let s = random_string(64).unwrap();
        assert_eq!(s.len(), 64);
        assert!(
            s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_ne!(s, random_string(64).unwrap());
    }

    #[tokio::test]
    async fn anilist_token_arrives_through_the_fragment_page() {
        // Port 0: let the OS pick, then build the config from the real port.
        let server = LoopbackServer::bind(0).await.unwrap();
        let port = server.port();
        drop(server);
        let login = PendingLogin::start(&config(port), ServiceId::AniList)
            .await
            .unwrap();
        assert!(login.authorize_url().starts_with(anilist::AUTHORIZE_URL));
        assert!(login.authorize_url().contains("response_type=token"));

        let browser = tokio::spawn(async move {
            assert!(get(port, "/favicon.ico").await.starts_with("HTTP/1.1 404"));
            let page = get(port, "/callback/anilist").await;
            assert!(page.contains("location.replace"));
            get(
                port,
                "/callback/anilist?access_token=tok&token_type=Bearer&expires_in=100",
            )
            .await
        });
        let credentials = login
            .finish_with(Duration::from_secs(5), "unused")
            .await
            .unwrap();
        assert_eq!(credentials.access_token, "tok");
        assert!(credentials.expires_at.is_some());
        assert!(browser.await.unwrap().contains("logged in"));
    }

    #[tokio::test]
    async fn myanimelist_rejects_wrong_state() {
        let server = LoopbackServer::bind(0).await.unwrap();
        let port = server.port();
        drop(server);
        let login = PendingLogin::start(&config(port), ServiceId::MyAnimeList)
            .await
            .unwrap();
        let url = Url::parse(login.authorize_url()).unwrap();
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["code_challenge_method"], "plain");
        assert_eq!(
            query["redirect_uri"],
            format!("http://localhost:{port}/callback/myanimelist")
        );

        tokio::spawn(async move { get(port, "/callback/myanimelist?code=c&state=wrong").await });
        let error = login
            .finish_with(Duration::from_secs(5), "unused")
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Auth(_)));
    }

    #[tokio::test]
    async fn cancelled_login_is_an_error() {
        let server = LoopbackServer::bind(0).await.unwrap();
        let port = server.port();
        drop(server);
        let login = PendingLogin::start(&config(port), ServiceId::MyAnimeList)
            .await
            .unwrap();
        tokio::spawn(async move { get(port, "/callback/myanimelist?error=access_denied").await });
        let error = login
            .finish_with(Duration::from_secs(5), "unused")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("access_denied"));
    }
}
