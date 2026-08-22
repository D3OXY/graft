use std::env;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let first = env::args().nth(1);
    match first.as_deref() {
        Some("--version") | Some("-V") | Some("version") | None => {
            println!("graft-hub {VERSION}");
        }
        Some("--help") | Some("-h") => {
            println!("graft-hub {VERSION}\n\nUsage: graft-hub [--data-dir <path>]");
        }
        Some(command) => {
            eprintln!("graft-hub: unexpected argument `{command}`");
            std::process::exit(2);
        }
    }
}
