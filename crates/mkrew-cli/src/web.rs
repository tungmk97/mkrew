//! `mk web` (ADR 0052): serves the phone page and relays its WebSocket to the daemon. Listens
//! on 127.0.0.1 by default; `tailscale serve` publishes it with HTTPS inside the user's
//! tailnet. The daemon itself never listens on the network (SPEC §15).

mod http;
mod push;
mod relay;
mod token;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use mkrew_core::Paths;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Role;

pub struct Args {
    pub listen: SocketAddr,
    pub host: Option<String>,
    pub new_token: bool,
}

const HEAD_LIMIT: usize = 8 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);

pub fn run(paths: &Paths, args: Args) -> anyhow::Result<ExitCode> {
    // After a reboot the phone may be the only way in: make sure there is a daemon.
    if crate::daemon::try_connect(paths)?.is_none() {
        crate::daemon::start(paths)?;
    }
    let host = args.host.or_else(mkrew_core::tailscale::dns_name);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("start the async runtime")?;
    runtime.block_on(serve(
        args.listen,
        host,
        paths.socket.clone(),
        paths.config_dir.clone(),
        paths.settings_file(),
        args.new_token,
    ))
}

/// What every connection handler shares.
struct Shared {
    socket: String,
    token: token::Token,
    /// `None` when the push store could not be opened: the page is still served, without push.
    push: Option<Arc<push::Push>>,
}

/// A subscription is a few hundred bytes; anything bigger is not from the page.
const BODY_LIMIT: usize = 4096;

async fn serve(
    listen: SocketAddr,
    host: Option<String>,
    socket: String,
    config_dir: PathBuf,
    settings_file: PathBuf,
    new_token: bool,
) -> anyhow::Result<ExitCode> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("listen on {listen}"))?;
    // After the bind: a second `mk web --new-token` must not replace the token of the running one.
    let token = token::load_or_create(&config_dir.join("web_token"), new_token)?;
    // `--new-token` revokes the old links, so it also forgets the phones that got push through them.
    // A store that cannot be opened must not take the page down with it.
    let push = match push::Push::open(
        &config_dir.join("web_push.json"),
        settings_file,
        new_token,
        push::sender_from_env(),
    ) {
        Ok(push) => Some(Arc::new(push)),
        Err(e) => {
            eprintln!("mk web: phone notifications are unavailable: {e:#}");
            None
        }
    };
    let addr = listener.local_addr()?;
    println!("Mkrew web is listening on http://{addr}");
    match &host {
        Some(host) => println!("Open on your phone: https://{host}/#t={}", token.as_str()),
        None => {
            println!(
                "Open on your phone: https://<this-pc>.<your-tailnet>.ts.net/#t={}",
                token.as_str()
            );
            println!(
                "(Tailscale's name for this PC was not found; pass --host <name> for the exact link.)"
            );
        }
    }
    println!(
        "First time on this PC: run `tailscale serve --bg {}` so the link works on your tailnet.",
        addr.port()
    );
    if let Some(push) = &push {
        if push.enabled() {
            println!(
                "Phone notifications are on (ADR 0066): phones that tapped the bell are notified through their browser's push service."
            );
        }
        tokio::spawn(push::run(Arc::clone(push), socket.clone()));
    }
    let shared = Arc::new(Shared {
        socket,
        token,
        push,
    });
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                eprintln!("mk web: accept: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            // A client that leaves mid-request needs no report; a relay that ends says why.
            if let Err(e) = handle(stream, &shared).await {
                eprintln!("mk web: {e:#}");
            }
        });
    }
}

async fn handle(mut stream: TcpStream, shared: &Shared) -> anyhow::Result<()> {
    let (head, rest) = tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut stream))
        .await
        .context("the request head did not arrive in time")??;
    let parsed = http::parse_head(&head);
    let route = parsed.as_ref().map_or(http::Route::BadRequest, |head| {
        http::route(head, &shared.token)
    });
    let length = parsed.map_or(0, |head| head.content_length);
    match &route {
        http::Route::Upgrade { key } => {
            // Connect first: a browser must never see an open WebSocket that dies at once.
            let daemon = match relay::open_daemon(&shared.socket).await {
                Ok(daemon) => daemon,
                Err(e) => {
                    stream
                        .write_all(&http::unavailable(&format!("{e:#}")))
                        .await?;
                    stream.shutdown().await?;
                    return Err(e);
                }
            };
            stream.write_all(&http::switching_protocols(key)).await?;
            let ws = WebSocketStream::from_partially_read(stream, rest, Role::Server, None).await;
            return relay::relay(ws, daemon).await;
        }
        http::Route::PushKey => {
            let reply = match shared.push.as_deref().and_then(push::Push::public_key) {
                Some(key) => http::plain("200 OK", &key),
                None => answer(push::Reply::Off),
            };
            stream.write_all(&reply).await?;
        }
        http::Route::PushSubscribe | http::Route::PushUnsubscribe => {
            let reply = if length > BODY_LIMIT {
                push::Reply::Bad("the request is too large".into())
            } else {
                let body = read_body(&mut stream, rest, length).await?;
                match (&shared.push, &route) {
                    (Some(push), http::Route::PushSubscribe) => push.subscribe(&body),
                    (Some(push), _) => push.unsubscribe(&body),
                    (None, _) => push::Reply::Off,
                }
            };
            stream.write_all(&answer(reply)).await?;
        }
        _ => stream.write_all(&route.response()).await?,
    }
    stream.shutdown().await?;
    Ok(())
}

/// The answer to a push request: 204 like `/check`, 404 while push is off, 400 with the reason.
fn answer(reply: push::Reply) -> Vec<u8> {
    match reply {
        push::Reply::Done => http::Route::Check.response(),
        push::Reply::Off => {
            http::plain("404 Not Found", "phone notifications are off on this PC\n")
        }
        push::Reply::Bad(why) => http::plain("400 Bad Request", &format!("{why}\n")),
    }
}

/// `length` bytes of request body: what arrived with the head, then the rest.
async fn read_body(
    stream: &mut TcpStream,
    mut body: Vec<u8>,
    length: usize,
) -> anyhow::Result<Vec<u8>> {
    let mut chunk = [0u8; 1024];
    while body.len() < length {
        let n = tokio::time::timeout(HEAD_TIMEOUT, stream.read(&mut chunk))
            .await
            .context("the request body did not arrive in time")??;
        if n == 0 {
            bail!("the connection closed before the request body ended");
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Ok(body)
}

/// The request head and whatever arrived after it.
async fn read_head(stream: &mut TcpStream) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = http::head_end(&buf) {
            let rest = buf.split_off(end);
            return Ok((buf, rest));
        }
        if buf.len() > HEAD_LIMIT {
            bail!("request head over {HEAD_LIMIT} bytes");
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            bail!("the connection closed before the request head ended");
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}
