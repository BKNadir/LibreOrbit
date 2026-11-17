//! `libreorbit <service> …`: one executable for every LibreOrbit service.
//!
//! ```text
//! libreorbit web <log_folder> [--profile=<path>] [--session=<key>] [--address=<ip>] [--chat=<host:port>]
//!                             [--uid=<id>] [--remote=<host:port>]       the web login (web.py's arguments)
//! ```
//!
//! Environment: `LO_ROOT` (installation folder: data/, cdn/), `OO_SAVE_DIR` (saves), `OO_WORLD` (World Editor world).

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((service, rest)) = args.split_first() else {
        eprintln!("usage: libreorbit web <log_folder> [options]");
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
        "--version" | "version" => {
            println!("libreorbit {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown service {other:?} (available: web)");
            ExitCode::from(2)
        }
    }
}
