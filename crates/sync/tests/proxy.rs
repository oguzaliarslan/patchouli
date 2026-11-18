//! Requests go through the proxy set with `http::set_proxy`.

use patchouli_sync::http::{self, ProxyConfig, ProxyKind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Accepts one connection, returns the request head and answers `ok`.
async fn fake_proxy(listener: TcpListener) -> String {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut buffer = vec![0; 4096];
    let mut head = Vec::new();
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = socket.read(&mut buffer).await.unwrap();
        if n == 0 {
            break;
        }
        head.extend_from_slice(&buffer[..n]);
    }
    socket
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
        .await
        .unwrap();
    String::from_utf8_lossy(&head).into_owned()
}

#[tokio::test]
async fn requests_use_the_configured_proxy() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let proxy = tokio::spawn(fake_proxy(listener));

    http::set_proxy(Some(ProxyConfig {
        kind: ProxyKind::Http,
        host: "127.0.0.1".into(),
        port: Some(port),
        username: "me".into(),
        password: "secret".into(),
    }));
    let body = http::client()
        .get("http://anime.example.invalid/list?page=1")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "ok");

    let head = proxy.await.unwrap();
    // An absolute URL in the request line means it was sent to a proxy.
    assert!(
        head.starts_with("GET http://anime.example.invalid/list?page=1 HTTP/1.1\r\n"),
        "{head}"
    );
    // `me:secret`, Base64.
    assert!(
        head.to_ascii_lowercase()
            .contains("proxy-authorization: basic bwu6c2vjcmv0"),
        "{head}"
    );

    // Unset: back to the system proxy, and a new client.
    http::set_proxy(None);
    assert_eq!(http::proxy(), None);
}
