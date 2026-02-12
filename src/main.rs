mod cmd;
mod error;
mod indexer;
mod schema;
mod searcher;

use clap::Parser;
use cmd::{Cli, Command, SearchArgs};

fn main() {
    let cli = Cli::parse();

    match &cli.command {
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
