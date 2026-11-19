//! `libreorbit <service> …`: one executable for every LibreOrbit service.
//!
//! ```text
//! libreorbit web <log_folder> [--profile=<path>] [--session=<key>] [--address=<ip>] [--chat=<host:port>]
//!                             [--uid=<id>] [--remote=<host:port>]       the web login (web.py's arguments)
//! libreorbit game <log_folder> [--profile=<path>] [--settings=<path>] [--stop-file=<file>] …
//!                                                                       the game server (game.py's arguments; in progress)
//! ```
//!
//! Environment: `LO_ROOT` (installation folder: data/, cdn/), `OO_SAVE_DIR` (saves), `OO_WORLD` (World Editor world).

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((service, rest)) = args.split_first() else {
        eprintln!("usage: libreorbit web|game <log_folder> [options]");
        return ExitCode::from(2);
    };
    match service.as_str() {
        "web" => {
            let cfg = match lo_web::Config::from_args(rest) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
            let web = match lo_web::Web::new(cfg, lo_core::Layout::from_env()) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("log folder: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            match rt.block_on(lo_web::run(web)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "game" => {
            let cfg = match lo_game::Config::from_args(rest) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
            let layout = lo_core::Layout::from_env();
            let spec = match lo_protocol::Spec::load(layout.data.join("protocol_read.json")) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("protocol: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let game = match lo_game::Game::new(cfg, spec, &layout, rest) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            };
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            match rt.block_on(lo_game::run(game)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "--version" | "version" => {
            println!("libreorbit {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown service {other:?} (available: web, game)");
            ExitCode::from(2)
        }
    }
}
