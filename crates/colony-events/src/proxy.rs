//! `colony-events proxy`: the only way out of a run's container.
//!
//! A run's container sits on an internal podman network with no route out. This proxy sits
//! on that network and the outside one, and lets through only the hosts the agent was
//! granted. It speaks HTTP proxy: `CONNECT host:port` (HTTPS) and absolute-form requests
//! (`GET http://host/...`). Each request's verdict is one JSON line on stdout,
//! `{"host", "port", "allowed"}`, which the daemon reads back into the event log when the
//! run ends.
//!
//! ```text
//! colony-events proxy --listen 0.0.0.0:3128 --allow news.ycombinator.com,*.reddit.com
//! ```

use std::io::Write;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Longest request head read before giving up on it.
const HEAD_MAX: usize = 16 * 1024;

/// Run the proxy until the process is killed. `args` follow `proxy`.
pub async fn main(args: &[String]) -> std::io::Result<()> {
    let mut listen = "0.0.0.0:3128".to_string();
    let mut allow = Vec::new();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it.next().cloned().unwrap_or_default();
        match flag.as_str() {
            "--listen" => listen = value,
            "--allow" => {
                allow = value
                    .split(',')
                    .filter(|h| !h.is_empty())
                    .map(str::to_ascii_lowercase)
                    .collect()
            }
            other => return Err(std::io::Error::other(format!("unknown option {other}"))),
        }
    }
    let listener = TcpListener::bind(&listen).await?;
    let allow = std::sync::Arc::new(allow);
    loop {
        let (client, _) = listener.accept().await?;
        let allow = std::sync::Arc::clone(&allow);
        tokio::spawn(async move {
            let _ = serve(client, &allow).await;
        });
    }
}

/// Whether `host` is granted: named exactly, or under a `*.domain` grant.
pub fn granted(allow: &[String], host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    allow.iter().any(|g| match g.strip_prefix("*.") {
        Some(domain) => host.ends_with(&format!(".{domain}")),
        None => *g == host,
    })
}

/// The target of a request head: `(host, port, is_connect)`.
fn target(head: &str) -> Option<(String, u16, bool)> {
    let mut parts = head.lines().next()?.split_whitespace();
    let method = parts.next()?;
    let uri = parts.next()?;
    if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = uri.rsplit_once(':')?;
        return Some((
            host.trim_matches(['[', ']']).to_string(),
            port.parse().ok()?,
            true,
        ));
    }
    let rest = uri.strip_prefix("http://")?;
    let authority = rest.split('/').next()?;
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (authority, 80),
    };
    Some((host.to_string(), port, false))
}

/// A proxy request head with its absolute URI cut to a path, as a server expects it.
fn origin_form(head: &str) -> String {
    let (line, rest) = head.split_once("\r\n").unwrap_or((head, ""));
    let mut parts = line.splitn(3, ' ');
    let (Some(method), Some(uri), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return head.to_string();
    };
    let path = uri
        .strip_prefix("http://")
        .and_then(|r| r.find('/').map(|i| &r[i..]))
        .unwrap_or("/");
    format!("{method} {path} {version}\r\n{rest}")
}

fn verdict(host: &str, port: u16, allowed: bool) {
    let line = serde_json::json!({ "host": host, "port": port, "allowed": allowed });
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

async fn serve(mut client: TcpStream, allow: &[String]) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(1024);
    let head_end = loop {
        let mut chunk = [0u8; 2048];
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > HEAD_MAX {
            return client
                .write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\n\r\n")
                .await;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let Some((host, port, connect)) = target(&head) else {
        return client.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
    };
    let allowed = granted(allow, &host);
    verdict(&host, port, allowed);
    if !allowed {
        let body = format!("{host} is not granted to this run\n");
        let resp = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        return client.write_all(resp.as_bytes()).await;
    }
    let Ok(mut upstream) = TcpStream::connect((host.as_str(), port)).await else {
        return client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
    };
    if connect {
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        upstream.write_all(&buf[head_end..]).await?;
    } else {
        upstream.write_all(origin_form(&head).as_bytes()).await?;
        upstream.write_all(&buf[head_end..]).await?;
    }
    tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_granted_hosts_pass() {
        let allow = vec![
            "news.ycombinator.com".to_string(),
            "*.reddit.com".to_string(),
        ];
        assert!(granted(&allow, "news.ycombinator.com"));
        assert!(granted(&allow, "NEWS.ycombinator.com"));
        assert!(granted(&allow, "old.reddit.com"));
        assert!(!granted(&allow, "reddit.com"), "*.x names subdomains only");
        assert!(!granted(&allow, "evilreddit.com"));
        assert!(!granted(&allow, "ycombinator.com"));
        assert!(!granted(&[], "example.com"));
    }

    #[test]
    fn the_target_comes_from_the_request_line() {
        assert_eq!(
            target("CONNECT example.com:443 HTTP/1.1\r\nHost: x\r\n\r\n"),
            Some(("example.com".into(), 443, true))
        );
        assert_eq!(
            target("GET http://example.com:8080/a HTTP/1.1\r\n\r\n"),
            Some(("example.com".into(), 8080, false))
        );
        assert_eq!(
            target("GET http://example.com/a HTTP/1.1\r\n\r\n"),
            Some(("example.com".into(), 80, false))
        );
        assert_eq!(
            target("GET /a HTTP/1.1\r\n\r\n"),
            None,
            "not a proxy request"
        );
    }

    #[test]
    fn a_forwarded_request_names_only_its_path() {
        assert_eq!(
            origin_form("GET http://example.com:8080/a?b HTTP/1.1\r\nHost: example.com\r\n\r\n"),
            "GET /a?b HTTP/1.1\r\nHost: example.com\r\n\r\n"
        );
        assert_eq!(
            origin_form("GET http://example.com HTTP/1.1\r\n\r\n"),
            "GET / HTTP/1.1\r\n\r\n"
        );
    }
}
