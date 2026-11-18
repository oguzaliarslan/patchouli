//! The HTTP client every request goes through, with the proxy from settings
//! (`network.proxy.*`, as in Taiga's `taiga/http.cpp`).
//!
//! Without a proxy set, the system proxy is used (reqwest reads the Windows
//! settings). [`set_proxy`] rebuilds the shared client; clients made before
//! the change keep the old proxy, so long-lived holders (the AniList and
//! MyAnimeList sessions) must be made again.

use std::sync::RwLock;
use std::time::Duration;

const USER_AGENT: &str = concat!("Patchouli/", env!("CARGO_PKG_VERSION"));
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProxyKind {
    #[default]
    Http,
    Socks5,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxyConfig {
    pub kind: ProxyKind,
    /// A host name or address, optionally with a scheme or `:port`.
    pub host: String,
    /// Overrides a port in `host`.
    pub port: Option<u16>,
    pub username: String,
    pub password: String,
}

impl ProxyConfig {
    /// The proxy URL without credentials, e.g. `socks5h://proxy.local:1080`.
    /// `None` if the host is empty or not a valid host.
    pub fn url(&self) -> Option<reqwest::Url> {
        let host = self.host.trim();
        if host.is_empty() {
            return None;
        }
        // Taiga v1 stored a URL; v2 a host.
        let rest = host.split_once("://").map_or(host, |(_, rest)| rest);
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let (name, port) = match authority.rsplit_once(':') {
            // Not an IPv6 address without a port (`[::1]`).
            Some((name, port)) if !port.contains(']') => (name, port.parse::<u16>().ok()),
            _ => (authority, None),
        };
        if name.is_empty() {
            return None;
        }
        let port = self.port.or(port).unwrap_or(match self.kind {
            ProxyKind::Http => 8080,
            ProxyKind::Socks5 => 1080,
        });
        // `socks5h`: the proxy resolves names, as Qt (and so Taiga) does.
        let scheme = match self.kind {
            ProxyKind::Http => "http",
            ProxyKind::Socks5 => "socks5h",
        };
        reqwest::Url::parse(&format!("{scheme}://{name}:{port}")).ok()
    }

    /// The URL with credentials, for clients that only take a URL.
    pub fn url_with_credentials(&self) -> Option<reqwest::Url> {
        let mut url = self.url()?;
        if !self.username.is_empty() {
            url.set_username(&self.username).ok()?;
            url.set_password(
                Some(&self.password)
                    .filter(|p| !p.is_empty())
                    .map(String::as_str),
            )
            .ok()?;
        }
        Some(url)
    }
}

static PROXY: RwLock<Option<ProxyConfig>> = RwLock::new(None);
static CLIENT: RwLock<Option<reqwest::Client>> = RwLock::new(None);

/// Sets the proxy for clients made from now on. `None`: the system proxy.
pub fn set_proxy(proxy: Option<ProxyConfig>) {
    let proxy = proxy.filter(|p| p.url().is_some());
    *PROXY.write().unwrap_or_else(|e| e.into_inner()) = proxy;
    *CLIENT.write().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn proxy() -> Option<ProxyConfig> {
    PROXY.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A client builder with the user agent, a 30 s timeout and the proxy.
pub fn builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(TIMEOUT);
    let Some(config) = proxy() else {
        return builder;
    };
    let Some(url) = config.url() else {
        return builder;
    };
    match reqwest::Proxy::all(url.as_str()) {
        Ok(mut proxy) => {
            if !config.username.is_empty() {
                proxy = proxy.basic_auth(&config.username, &config.password);
            }
            builder.proxy(proxy)
        }
        Err(_) => builder,
    }
}

/// The shared client (cheap to clone).
pub fn client() -> reqwest::Client {
    if let Some(client) = CLIENT.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return client.clone();
    }
    let client = builder().build().unwrap_or_default();
    *CLIENT.write().unwrap_or_else(|e| e.into_inner()) = Some(client.clone());
    client
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(kind: ProxyKind, host: &str, port: Option<u16>) -> ProxyConfig {
        ProxyConfig {
            kind,
            host: host.into(),
            port,
            ..ProxyConfig::default()
        }
    }

    #[test]
    fn proxy_urls() {
        let url = |c: ProxyConfig| c.url().map(|u| u.to_string());
        assert_eq!(
            url(config(ProxyKind::Http, "proxy.local", Some(3128))).as_deref(),
            Some("http://proxy.local:3128/")
        );
        assert_eq!(
            url(config(ProxyKind::Http, "proxy.local", None)).as_deref(),
            Some("http://proxy.local:8080/")
        );
        // Taiga v1 stored a URL with the port.
        assert_eq!(
            url(config(ProxyKind::Socks5, "socks5://10.0.0.2:9050", None)).as_deref(),
            Some("socks5h://10.0.0.2:9050")
        );
        assert_eq!(url(config(ProxyKind::Http, "  ", Some(80))), None);
        let with_login = ProxyConfig {
            username: "me".into(),
            password: "p@ss".into(),
            ..config(ProxyKind::Http, "proxy.local", Some(3128))
        };
        assert_eq!(
            with_login.url_with_credentials().unwrap().as_str(),
            "http://me:p%40ss@proxy.local:3128/"
        );
    }
}
