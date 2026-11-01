//! Entry point. With no arguments the helper runs in protocol mode (JSON commands on
//! stdin, binary frames on stdout) as the desktop core's sidecar. `record` is a debug
//! subcommand; `--help` prints usage. See packages/shared/CAPTURE_HELPER.md.

use hearme_windows_capture::usage_text;

#[cfg(windows)]
fn main() {
    use hearme_windows_capture::recorder::{self, Options};
    use hearme_windows_capture::session::Session;
    use std::path::PathBuf;

    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("record") => {
            let Some(options) = parse_record_options(&args[1..]) else {
                eprintln!("usage: tapcapture record --process <exe> [--seconds <n>] [--out <dir>]");
                std::process::exit(2);
            };
            std::process::exit(recorder::run(options));
        }
        Some("--help") | Some("-h") | Some("help") => {
            println!("{}", usage_text());
        }
        None => {
            // Protocol mode: runs until stdin EOF / shutdown.
            Session::new().run();
        }
        Some(unknown) => {
            eprintln!("unknown argument: {unknown}");
            println!("{}", usage_text());
            std::process::exit(2);
        }
    }

    fn parse_record_options(args: &[String]) -> Option<Options> {
        let mut process: Option<String> = None;
        let mut seconds = 30.0;
        let mut out = PathBuf::from("./out");
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--process" => {
                    i += 1;
                    process = args.get(i).cloned();
                }
                "--seconds" => {
                    i += 1;
                    if let Some(v) = args.get(i).and_then(|s| s.parse::<f64>().ok()) {
                        seconds = v;
                    }
                }
                "--out" => {
                    i += 1;
                    if let Some(v) = args.get(i) {
                        out = PathBuf::from(v);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        process.map(|process| Options {
            process,
            seconds,
            out_dir: out,
        })
    }
}

#[cfg(not(windows))]
fn main() {
    // The capture implementation is Windows-only; on other platforms the binary just
    // prints usage so the workspace still builds.
    println!("{}", usage_text());
}
