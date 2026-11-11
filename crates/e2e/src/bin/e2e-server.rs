//! `e2e-server <entry> <port> [threads]`
//!
//! Serves every route of `entry` over TCP on `127.0.0.1:<port>` through
//! Kynos's own server, for an external load generator: a multi-thread tokio
//! runtime with `threads` workers (default 4). Port 0 picks a free port.
//! Prints `ready <port>` once listening and exits 0 on SIGTERM or ctrl-c.
//! Exit 2 is a usage error, 3 an unknown entry.

use std::io::Write;
use std::process::ExitCode;

use kynos::server::Server;

fn usage() -> ExitCode {
    eprintln!("usage: e2e-server <entry> <port> [threads]");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [entry, port, rest @ ..] = args.as_slice() else {
        return usage();
    };
    let Ok(port) = port.parse::<u16>() else {
        return usage();
    };
    let threads = match rest.first().map(|t| t.parse::<usize>()) {
        None => 4,
        Some(Ok(threads)) if threads > 0 => threads,
        Some(_) => return usage(),
    };
    let Some(service) = e2e::entry::service(entry) else {
        eprintln!(
            "entry {entry} is not compiled into this build: {:?}",
            e2e::entry::compiled()
        );
        return ExitCode::from(3);
    };
    let service = match service {
        Ok(service) => service,
        Err(error) => {
            eprintln!("router: {error}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(service, port)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("e2e-server: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(service: kynos::router::service::Service<()>, port: u16) -> Result<(), String> {
    let bound = Server::new(service)
        .bind((std::net::Ipv4Addr::LOCALHOST, port))
        .prepare()
        .await
        .map_err(|error| error.to_string())?;
    let port = bound
        .local_addrs()
        .first()
        .map(std::net::SocketAddr::port)
        .ok_or("no listener")?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "ready {port}").map_err(|error| error.to_string())?;
    stdout.flush().map_err(|error| error.to_string())?;
    drop(stdout);
    tokio::select! {
        served = bound.serve() => served.map_err(|error| error.to_string()),
        () = shutdown() => Ok(()),
    }
}

/// Resolves on the first SIGTERM or ctrl-c.
async fn shutdown() {
    use tokio::signal::unix::{SignalKind, signal};
    match signal(SignalKind::terminate()) {
        Ok(mut term) => {
            tokio::select! {
                _ = term.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
        }
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}
