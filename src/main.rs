// Declare module files that will be part of this crate
mod repository;  // Handles repository operations and state
mod objects;     // Defines Git object types and their handling
mod pack;        // Manages packfile operations (compressed Git objects)

// Import necessary standard library components
use std::error::Error;  // For generic error handling
use std::env;           // For accessing command-line arguments
use repository::Repository;  // Main repository type from our module

// Main function with Box<dyn Error> return for flexible error handling
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect(); // Collect CLI arguments into a vector of Strings
    
    let path = match args.as_slice() {       // Handle args using pattern matching
        // Case when only program name is provided (no arguments)
        [_,] => ".",  // Default to current directory
        
        
        [_, arg] if arg == "--version" || arg == "-v" => { // Version flag case
            println!("Version: {}", env!("CARGO_PKG_VERSION"));
            return Ok(());                  // Early return after version display
        },

        [_, arg] if arg == "--help" => {    // Version flag case
            println!(r#"
            rakke - Git repository statistics analyzer
            
            Usage:
                rakke [OPTIONS]
            
            Options:
                -v, --version     Show version information
                -h, --help        Print this help message
            
            Examples:
                rakke             Analyze current git repo
                rakke /repo/path/ Analyzes git repo in a given directory
            "#);

            return Ok(());  // Early return after version display
        },
        
        // Path provided case
        [_, path] => path,  // Use the provided path
           
        _ => {                    // Handle unexpected number of arguments
            eprintln!("Usage: rakke [PATH|--version]");
            return Err("Invalid arguments".into());
        }
    };

    // Attempt to initialize repository at the specified path
    let repo = match Repository::new(path) {
        Ok(repo) => repo,  // Success case - store the repository
        Err(e) => {           // Error case - print and propagate error
            eprintln!("Error initializing repository: {}", e);
            return Err(e);
        }
    };

    // Try to count all commits in the repository
    match repo.count_all_commits() {
        Ok(count) => {  // Success case - print commit count
            println!("Total commits in repository: {}", count);
            Ok(())      // Return success
        },
        Err(e) => {     // Error case - print and propagate error
            eprintln!("Error counting commits: {}", e);
            Err(e)
        }
    }
}