//! The reports viewport: a web page the `colony-events` daemon serves, usable on a phone.
//!
//! It shows what needs you first (blocked reports, and reports with an ask nobody has
//! answered), then the feed of every report, newest first. Each evidence address opens the
//! exact output it names. Answering an ask here resumes the run that asked it.
//!
//! | Route | What it returns |
//! |---|---|
//! | `GET /` | the page |
//! | `GET /app.js` | its script |
//! | `GET /api/feed` | `{needs_you: [Item], feed: [Item]}` |
//! | `GET /open?a=<colony://...>` | an output's or diff's exact content as `text/plain`, or an event as JSON |
//! | `POST /api/answer` | `{ask, answer}` answers an ask; the log records the device's IP as who |
//!
//! It binds `COLONY_VIEW_ADDR`, default `127.0.0.1:7337`: only this machine. To open it on
//! your phone, bind the machine's address on your private network (its Tailscale IP, say).
//! The page has no login; the private network is the boundary, and the log names each
//! answer's device by its address on that network.
//!
//! A `POST` must carry an `Origin` naming this server and a JSON body, so another web
//! page in your browser cannot answer for you: its own origin fails the check, and a
//! cross-origin JSON post needs a CORS preflight this server never grants.
//!
//! Report text comes from a model, so the page builds every node with `textContent`
//! (never `innerHTML`) and the CSP allows no inline script.
//!
//! Every request must name the bound address in its `Host` header (`localhost:<port>` is
//! also accepted on loopback), else 421. That stops DNS rebinding: a web page whose own
//! name resolves to 127.0.0.1 still sends its own name as `Host`.

use std::net::SocketAddr;
use std::sync::Arc;

use std::collections::HashMap;

use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use colony_tuple::{Address, EventLog, Scope};
use serde::{Deserialize, Serialize};

use crate::agents::{answer_ask, Report, ReportStatus, ASK_ANSWERED, REPORTS_VIEW, REPORT_POSTED};

/// Where the viewport listens unless `COLONY_VIEW_ADDR` says otherwise.
pub const DEFAULT_ADDR: &str = "127.0.0.1:7337";

/// Most reports the feed returns.
const FEED_MAX: usize = 200;

const PAGE: &str = include_str!("viewport/index.html");
const SCRIPT: &str = include_str!("viewport/app.js");
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; \
                   connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; \
                   frame-ancestors 'none'";

/// The address to bind: `COLONY_VIEW_ADDR`, else [`DEFAULT_ADDR`].
pub fn bind_addr() -> Result<SocketAddr, String> {
    let raw = std::env::var("COLONY_VIEW_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    raw.parse()
        .map_err(|e| format!("COLONY_VIEW_ADDR '{raw}' is not host:port ({e})"))
}

/// Where the daemon writes the viewport's URL, for `on --daemon status`.
pub fn url_file() -> std::path::PathBuf {
    crate::events::daemon_pid_path().with_file_name("viewport.url")
}

/// Serve the viewport until the task is dropped. `log_path` is the machine's SQLite file.
pub async fn serve(addr: SocketAddr, log_path: std::path::PathBuf) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    let url = format!("http://{bound}/");
    let _ = std::fs::write(url_file(), &url);
    tracing::info!("Viewport at {url}");
    let app = router(Arc::new(log_path), allowed_hosts(bound));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

/// The `Host` values a request may carry for a server bound to `bound`.
// ponytail: a MagicDNS or other name for the machine is refused; add COLONY_VIEW_HOSTS if needed
fn allowed_hosts(bound: SocketAddr) -> Arc<Vec<String>> {
    let mut hosts = vec![bound.to_string()];
    if bound.ip().is_loopback() {
        hosts.push(format!("localhost:{}", bound.port()));
    }
    Arc::new(hosts)
}

fn router(log_path: Arc<std::path::PathBuf>, hosts: Arc<Vec<String>>) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/app.js", get(script))
        .route("/api/feed", get(feed))
        .route("/open", get(open))
        .route("/api/answer", post(answer))
        .with_state(log_path)
        .layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let hosts = Arc::clone(&hosts);
                async move {
                    let host = req
                        .headers()
                        .get(header::HOST)
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or_default();
                    if hosts.iter().any(|h| h.eq_ignore_ascii_case(host)) {
                        next.run(req).await
                    } else {
                        error(StatusCode::MISDIRECTED_REQUEST, "unknown Host")
                    }
                }
            },
        ))
}

/// One report in the feed.
#[derive(Debug, Serialize)]
pub struct Item {
    /// The run's address.
    pub address: String,
    pub agent: String,
    pub run: u64,
    /// When it was posted, Unix ms.
    pub at: i64,
    pub headline: String,
    pub status: ReportStatus,
    pub body: String,
    pub evidence: Vec<String>,
    pub asks: Vec<AskItem>,
}

/// One ask in a report, and its answer once it has one.
#[derive(Debug, Clone, Serialize)]
pub struct AskItem {
    pub address: String,
    pub question: String,
    pub options: Vec<String>,
    pub answer: Option<String>,
    /// Who answered.
    pub by: Option<String>,
    /// When, Unix ms.
    pub answered_at: Option<i64>,
}

/// What the page shows: what needs you, then everything.
#[derive(Debug, Serialize)]
pub struct Feed {
    pub needs_you: Vec<Item>,
    pub feed: Vec<Item>,
}

/// The reports viewport's contents, from the log.
pub fn read_feed(log: &EventLog) -> Result<Feed, String> {
    let events = log
        .of_kind(REPORT_POSTED, FEED_MAX)
        .map_err(|e| e.to_string())?;
    // Oldest first, so the first answer to each ask wins, as in the agent's fold.
    let mut answers: HashMap<String, (String, String, i64)> = HashMap::new();
    for ev in log
        .of_kind(ASK_ANSWERED, FEED_MAX * 3)
        .map_err(|e| e.to_string())?
        .iter()
        .rev()
    {
        let (Some(subject), Some(text)) = (ev.subject.clone(), ev.data["answer"].as_str()) else {
            continue;
        };
        answers
            .entry(subject)
            .or_insert_with(|| (text.to_string(), ev.actor.clone(), ev.at_ms));
    }
    let feed: Vec<Item> = events
        .iter()
        .filter_map(|ev| {
            let report: Report = serde_json::from_value(ev.data.clone()).ok()?;
            if report.view != REPORTS_VIEW {
                return None;
            }
            let addr: Address = ev.subject.as_deref()?.parse().ok()?;
            let Scope::Run { agent, run } = addr.scope.clone() else {
                return None;
            };
            Some(Item {
                address: addr.to_string(),
                agent,
                run,
                at: ev.at_ms,
                headline: report.headline,
                status: report.status,
                body: report.body,
                evidence: report.evidence,
                asks: report
                    .asks
                    .into_iter()
                    .map(|a| {
                        let answered = answers.get(&a.address);
                        AskItem {
                            answer: answered.map(|(t, _, _)| t.clone()),
                            by: answered.map(|(_, b, _)| b.clone()),
                            answered_at: answered.map(|(_, _, at)| *at),
                            address: a.address,
                            question: a.question,
                            options: a.options,
                        }
                    })
                    .collect(),
            })
        })
        .collect();
    let needs_you = feed
        .iter()
        .filter(|i| i.status == ReportStatus::Blocked || i.asks.iter().any(|a| a.answer.is_none()))
        .map(|i| Item {
            address: i.address.clone(),
            agent: i.agent.clone(),
            run: i.run,
            at: i.at,
            headline: i.headline.clone(),
            status: i.status,
            body: i.body.clone(),
            evidence: i.evidence.clone(),
            asks: i.asks.clone(),
        })
        .collect();
    Ok(Feed { needs_you, feed })
}

fn secure(mut resp: Response) -> Response {
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

async fn page() -> Response {
    secure(([(header::CONTENT_TYPE, "text/html; charset=utf-8")], PAGE).into_response())
}

async fn script() -> Response {
    secure(
        (
            [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
            SCRIPT,
        )
            .into_response(),
    )
}

fn open_log(path: &std::path::Path) -> Result<EventLog, Box<Response>> {
    EventLog::open(path)
        .map_err(|e| Box::new(error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())))
}

fn error(status: StatusCode, msg: &str) -> Response {
    secure(
        (
            status,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            msg.to_string(),
        )
            .into_response(),
    )
}

async fn feed(State(path): State<Arc<std::path::PathBuf>>) -> Response {
    let log = match open_log(&path) {
        Ok(log) => log,
        Err(resp) => return *resp,
    };
    match read_feed(&log) {
        Ok(feed) => secure(Json(feed).into_response()),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

#[derive(Deserialize)]
struct AnswerBody {
    ask: String,
    answer: String,
}

/// `POST /api/answer`. The `Host` check has already passed; the `Origin` must match it.
async fn answer(
    State(path): State<Arc<std::path::PathBuf>>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(body): Json<AnswerBody>,
) -> Response {
    let text = |h: header::HeaderName| headers.get(h).and_then(|v| v.to_str().ok());
    let same_origin = matches!(
        (text(header::ORIGIN), text(header::HOST)),
        (Some(origin), Some(host)) if origin.eq_ignore_ascii_case(&format!("http://{host}"))
    );
    if !same_origin {
        return error(StatusCode::FORBIDDEN, "cross-origin answers are refused");
    }
    let actor = peer.map_or_else(
        || "viewport".to_string(),
        |Extension(ConnectInfo(addr))| format!("viewport@{}", addr.ip()),
    );
    let log = match open_log(&path) {
        Ok(log) => log,
        Err(resp) => return *resp,
    };
    match answer_ask(&log, &actor, &body.ask, &body.answer) {
        Ok(seq) => secure(Json(serde_json::json!({ "seq": seq, "by": actor })).into_response()),
        Err(e) => error(StatusCode::CONFLICT, &e),
    }
}

#[derive(Deserialize)]
struct OpenQuery {
    a: String,
}

async fn open(State(path): State<Arc<std::path::PathBuf>>, Query(q): Query<OpenQuery>) -> Response {
    let addr: Address = match q.a.parse() {
        Ok(addr) => addr,
        Err(e) => return error(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    if !addr.is_local() {
        return error(StatusCode::NOT_FOUND, "only local addresses resolve");
    }
    let log = match open_log(&path) {
        Ok(log) => log,
        Err(resp) => return *resp,
    };
    let found = match &addr.scope {
        Scope::Output { .. } | Scope::Diff { .. } => log.latest(&addr).map(|ev| {
            ev.and_then(|ev| ev.data["content"].as_str().map(str::to_string))
                .map(|text| {
                    // Exact bytes, never rendered: text/plain with nosniff.
                    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response()
                })
        }),
        Scope::Event(seq) => log.get(*seq).map(|ev| {
            ev.map(|ev| {
                Json(serde_json::json!({
                    "seq": ev.seq, "at": ev.at_ms, "actor": ev.actor, "cause": ev.cause,
                    "kind": ev.kind, "subject": ev.subject, "data": ev.data,
                }))
                .into_response()
            })
        }),
        Scope::Agent(_) | Scope::Run { .. } | Scope::Ask { .. } | Scope::View(_) => log.about(&addr).map(|events| {
            (!events.is_empty()).then(|| {
                let events: Vec<_> = events
                    .iter()
                    .map(|ev| serde_json::json!({ "seq": ev.seq, "at": ev.at_ms, "kind": ev.kind, "subject": ev.subject }))
                    .collect();
                Json(events).into_response()
            })
        }),
    };
    match found {
        Ok(Some(resp)) => secure(resp),
        Ok(None) => error(StatusCode::NOT_FOUND, &format!("nothing at {addr}")),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::run_address;
    use serde_json::json;

    fn report(status: ReportStatus, headline: &str) -> serde_json::Value {
        serde_json::to_value(Report {
            headline: headline.into(),
            status,
            body: "b".into(),
            evidence: vec!["colony://local/agent/a/run/1/output/1".into()],
            asks: vec![],
            view: REPORTS_VIEW.into(),
        })
        .unwrap()
    }

    #[test]
    fn the_feed_is_newest_first_and_blocked_reports_need_you() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(&dir.path().join("m.db")).unwrap();
        log.append(
            "d",
            REPORT_POSTED,
            Some(&run_address("a", 1)),
            None,
            report(ReportStatus::Attention, "first"),
        )
        .unwrap();
        log.append(
            "d",
            REPORT_POSTED,
            Some(&run_address("a", 2)),
            None,
            report(ReportStatus::Blocked, "second"),
        )
        .unwrap();
        log.append("d", "Other", Some(&run_address("a", 3)), None, json!({}))
            .unwrap();

        let feed = read_feed(&log).unwrap();
        let heads: Vec<_> = feed.feed.iter().map(|i| i.headline.as_str()).collect();
        assert_eq!(heads, ["second", "first"]);
        assert_eq!(feed.needs_you.len(), 1);
        assert_eq!(feed.needs_you[0].address, "colony://local/agent/a/run/2");
        assert_eq!((feed.feed[0].agent.as_str(), feed.feed[0].run), ("a", 2));
    }

    #[test]
    fn the_page_never_puts_report_text_into_html() {
        assert!(!SCRIPT.contains("innerHTML"));
        assert!(!SCRIPT.contains("insertAdjacentHTML"));
        assert!(!SCRIPT.contains("document.write"));
        assert!(
            !PAGE.contains("<script>"),
            "no inline script: the CSP forbids it"
        );
    }

    #[tokio::test]
    async fn a_request_naming_another_host_is_refused() {
        use tower::ServiceExt;
        let dir = tempfile::tempdir().unwrap();
        let bound: SocketAddr = "127.0.0.1:7337".parse().unwrap();
        let app = router(Arc::new(dir.path().join("m.db")), allowed_hosts(bound));
        let get = |host: &str| {
            axum::http::Request::builder()
                .uri("/api/feed")
                .header(header::HOST, host)
                .body(axum::body::Body::empty())
                .unwrap()
        };
        for host in ["127.0.0.1:7337", "localhost:7337", "LOCALHOST:7337"] {
            let resp = app.clone().oneshot(get(host)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "{host}");
        }
        for host in ["evil.example:7337", "127.0.0.1:80", "localhost", ""] {
            let resp = app.clone().oneshot(get(host)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST, "{host}");
        }
    }

    #[test]
    fn a_report_with_an_open_ask_needs_you_until_it_is_answered() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(&dir.path().join("m.db")).unwrap();
        let mut r: Report = serde_json::from_value(report(ReportStatus::NeedsYou, "asks")).unwrap();
        let ask = "colony://local/agent/a/run/1/ask/1";
        r.asks = vec![crate::agents::Ask {
            address: ask.into(),
            question: "Send?".into(),
            options: vec!["yes".into(), "no".into()],
        }];
        log.append(
            "d",
            REPORT_POSTED,
            Some(&run_address("a", 1)),
            None,
            serde_json::to_value(r).unwrap(),
        )
        .unwrap();
        let feed = read_feed(&log).unwrap();
        assert_eq!(feed.needs_you.len(), 1);
        assert_eq!(feed.needs_you[0].asks[0].options, ["yes", "no"]);

        let ask_addr: Address = ask.parse().unwrap();
        for (who, answer) in [("viewport@100.64.0.2", "yes"), ("late", "no")] {
            log.append(
                who,
                ASK_ANSWERED,
                Some(&ask_addr),
                None,
                json!({"answer": answer}),
            )
            .unwrap();
        }
        let feed = read_feed(&log).unwrap();
        assert!(feed.needs_you.is_empty());
        let asked = &feed.feed[0].asks[0];
        assert_eq!(asked.answer.as_deref(), Some("yes"));
        assert_eq!(asked.by.as_deref(), Some("viewport@100.64.0.2"));
    }

    #[tokio::test]
    async fn an_answer_from_another_origin_is_refused() {
        use tower::ServiceExt;
        let dir = tempfile::tempdir().unwrap();
        let bound: SocketAddr = "127.0.0.1:7337".parse().unwrap();
        let app = router(Arc::new(dir.path().join("m.db")), allowed_hosts(bound));
        let post = |origin: Option<&str>| {
            let mut req = axum::http::Request::builder()
                .method("POST")
                .uri("/api/answer")
                .header(header::HOST, "127.0.0.1:7337")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(o) = origin {
                req = req.header(header::ORIGIN, o);
            }
            req.body(axum::body::Body::from(
                r#"{"ask":"colony://local/agent/a/run/1/ask/1","answer":"yes"}"#,
            ))
            .unwrap()
        };
        for origin in [None, Some("http://evil.example"), Some("null")] {
            let resp = app.clone().oneshot(post(origin)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::FORBIDDEN, "{origin:?}");
        }
        let resp = app
            .clone()
            .oneshot(post(Some("http://127.0.0.1:7337")))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::CONFLICT,
            "same origin reaches the ask check"
        );
    }

    #[test]
    fn the_default_address_is_this_machine_only() {
        let addr: SocketAddr = DEFAULT_ADDR.parse().unwrap();
        assert!(addr.ip().is_loopback());
    }
}
