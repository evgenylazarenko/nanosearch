mod cmd;
mod error;
mod indexer;
mod schema;
mod searcher;
mod stats;

use clap::Parser;
use cmd::{Cli, Command, SearchArgs};

fn main() {
    // Reset SIGPIPE to default (terminate silently) so piping works.
    // Rust runtime sets SIG_IGN, which causes panics on write to closed pipes.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Search(sub_args)) => {
            let args = SearchArgs::from_search_sub(sub_args, &cli);
            cmd::search::run(&args);
        }
        Some(Command::Index(args)) => cmd::index::run(args),
        Some(Command::Status) => cmd::status::run(),
        Some(Command::Hooks { action }) => cmd::hooks::run(action),
        None => {
            // Default mode: search
            match &cli.query {
                Some(query) => {
                    let args = SearchArgs::from_cli(&cli, query.clone());
                    cmd::search::run(&args);
                }
                None => {
                    // No query and no subcommand — show help
                    use clap::CommandFactory;
                    Cli::command().print_help().expect("failed to print help");
                    println!();
                }
            }
        }
    }
}
