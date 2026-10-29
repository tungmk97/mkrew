//! `mk proxy` (ADR 0089): joins stdin/stdout to the daemon socket, starting the daemon first
//! when needed. A machine window runs it on the other computer through `ssh`; the window sends
//! `hello` itself, so the proxy only moves bytes. Nothing but daemon bytes goes to stdout.

use std::io::{self, Read, Write};
use std::process::ExitCode;

use anyhow::Context;
use interprocess::local_socket::{Stream, prelude::*};
use mkrew_core::Paths;

const CHUNK: usize = 64 * 1024;

pub fn run(paths: &Paths) -> anyhow::Result<ExitCode> {
    crate::daemon::ensure_listening(paths)?;
    let stream = Stream::connect(mkrew_daemon::ipc::name(&paths.socket)?)
        .with_context(|| format!("connect to {}", paths.socket))?;
    let (mut from_daemon, mut to_daemon) = stream.split();
    // stdin → daemon on its own thread; daemon → stdout here. Either side ending ends the
    // proxy. Two blocking threads, not a select: racing pipe reads loses data on Windows.
    std::thread::Builder::new()
        .name("mk-proxy-in".into())
        .spawn(move || {
            let mut stdin = io::stdin().lock();
            let mut buf = vec![0u8; CHUNK];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if to_daemon.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                }
            }
            // The window (or ssh) went away. The main thread is blocked in a pipe read that
            // nothing else can interrupt portably, so end the process; that closes the socket.
            std::process::exit(0);
        })
        .context("start the stdin thread")?;
    let mut stdout = io::stdout().lock();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = from_daemon.read(&mut buf).context("read from the daemon")?;
        if n == 0 {
            return Ok(ExitCode::SUCCESS);
        }
        stdout.write_all(&buf[..n]).context("write to stdout")?;
        // Line-buffered stdout would hold binary frames until a newline.
        stdout.flush().context("write to stdout")?;
    }
}
