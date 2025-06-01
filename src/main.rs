use std::env;

mod init;
mod add;

fn main() {
    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    
    // Check if we have at least one command
    if args.len() < 2 {
        eprintln!("Usage: rakke <command>");
        eprintln!("Available commands: init, add, --version");
        return;
    }
    
    // Get first command after program name
    let command = &args[1];
    
    // Command dispatcher
    match command.as_str() {
        "init" => {
            // Pass arguments to init module for complete isolation
            let init_args: Vec<String> = args[1..].to_vec();
            init::execute(init_args);
        }
        "add" => {
            // Pass arguments to init module for complete isolation
            let init_args: Vec<String> = args[1..].to_vec();
            add::execute(init_args);
        }
        "--version" | "-v" => {
            // Show version information
            println!("rakke version {}", env!("CARGO_PKG_VERSION"));

        }
        _ => {
            // Unknown command
            eprintln!("Unknown command: {}", command);
            eprintln!("Available commands: init, --version");
        }
    }
}