//! The backend's dead-man's-switch (uptime.rs).

use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::support::*;
use crate::uptime::run_once;

/// A stand-in for healthchecks.io: answers every request with 200 and
/// hands back the request line's path.
async fn fake_healthchecks() -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/ping-uuid", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or_default().to_string();
            tx.send(path).unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nOK").await.unwrap();
        }
    });
    (url, rx)
}

#[sqlx::test(migrations = "./migrations")]
async fn pings_success_while_healthy_and_fail_when_the_database_is_gone(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (url, mut pings) = fake_healthchecks().await;
    let client = reqwest::Client::new();

    run_once(&app.state, &client, &url).await;
    assert_eq!(pings.recv().await.unwrap(), "/ping-uuid");

    app.state.db.close().await;
    run_once(&app.state, &client, &url).await;
    assert_eq!(pings.recv().await.unwrap(), "/ping-uuid/fail");
}
