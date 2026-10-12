//! `mk web` (ADR 0051): serves the phone page and relays its WebSocket to the daemon. Listens
//! on 127.0.0.1 by default; `tailscale serve` publishes it with HTTPS inside the user's
//! tailnet. The daemon itself never listens on the network (SPEC §15).

mod http;
mod relay;
mod tailscale;
mod token;

use std::net::SocketAddr;
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
    let token = token::load_or_create(&paths.config_dir.join("web_token"), args.new_token)?;
    // After a reboot the phone may be the only way in: make sure there is a daemon.
    if crate::daemon::try_connect(paths)?.is_none() {
        crate::daemon::start(paths)?;
    }
    let host = args.host.or_else(tailscale::dns_name);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("start the async runtime")?;
    runtime.block_on(serve(args.listen, host, paths.socket.clone(), token))
}

async fn serve(
    listen: SocketAddr,
    host: Option<String>,
    socket: String,
    token: token::Token,
) -> anyhow::Result<ExitCode> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("listen on {listen}"))?;
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
    let shared = Arc::new((socket, token));
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                eprintln!("mk web: accept: {e}");
                continue;
            }
        };
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            let (socket, token) = &*shared;
            // A client that leaves mid-request needs no report.
            let _ = handle(stream, socket, token).await;
        });
    }
}

async fn handle(mut stream: TcpStream, socket: &str, token: &token::Token) -> anyhow::Result<()> {
    let (head, rest) = tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut stream))
        .await
        .context("the request head did not arrive in time")??;
    let route =
        http::parse_head(&head).map_or(http::Route::BadRequest, |head| http::route(&head, token));
    if let http::Route::Upgrade { key } = &route {
        stream.write_all(&http::switching_protocols(key)).await?;
        let ws = WebSocketStream::from_partially_read(stream, rest, Role::Server, None).await;
        relay::relay(ws, socket).await;
        return Ok(());
    }
    stream.write_all(&route.response()).await?;
    stream.shutdown().await?;
    Ok(())
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
