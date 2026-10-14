mod cobra;
mod server;
mod worker;

use std::process::ExitCode;

const USAGE: &str = "\
Usage: cobra-mcp [--help | --version]

An MCP server for CoBRA that communicates over stdio. MCP clients start it
themselves; add it to your client's configuration, e.g. in .mcp.json:

    { \"mcpServers\": { \"cobra\": { \"command\": \"cobra-mcp\" } } }
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => serve(),
        [worker::FLAG] => worker::run(),
        ["--version" | "-V"] => {
            println!("cobra-mcp {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["--help" | "-h"] => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprint!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn serve() -> ExitCode {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start the async runtime");
    match runtime.block_on(server::serve()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cobra-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}
