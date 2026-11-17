use std::time::Duration;

/// HTTP client shared by the services.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Patchouli/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}
