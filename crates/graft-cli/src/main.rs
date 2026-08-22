use std::env;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let first = env::args().nth(1);
    match first.as_deref() {
        Some("--version") | Some("-V") | Some("version") => {
            println!("graft {VERSION}");
        }
        Some("--help") | Some("-h") | None => {
            println!("graft {VERSION}\n\nUsage: graft <command>\n\nRun `graft <command> --help` for command help.");
        }
        Some(command) => {
            eprintln!("graft: command `{command}` is not available yet");
            std::process::exit(2);
        }
    }
}
