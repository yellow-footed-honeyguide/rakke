use std::fs;
use std::io;
use std::path::Path;
use std::env;

pub fn execute(args: Vec<String>) {
    // Parse command line arguments
    let mut directory = ".".to_string();
    let mut bare = false;
    
    // Process arguments (skip "init" command itself)
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--bare" => bare = true,
            "--help" | "-h" => {
                print_help();
                return;
            }
            arg if !arg.starts_with('-') => {
                directory = arg.to_string();
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                print_help();
                return;
            }
        }
        i += 1;
    }
    
    // Execute initialization
    match initialize_repository(&directory, bare) {
        Ok(repo_path) => {
            if bare {
                println!("Initialized empty Git repository in {}", repo_path);
            } else {
                println!("Initialized empty Git repository in {}/.git/", repo_path);
            }
        }
        Err(e) => {
            eprintln!("fatal: {}", e);
            std::process::exit(1);
        }
    }
}

fn initialize_repository(directory: &str, bare: bool) -> Result<String, String> {
    // Get absolute path for output
    let current_dir = env::current_dir()
        .map_err(|e| format!("Cannot get current directory: {}", e))?;
    
    let target_path = if directory == "." {
        current_dir
    } else {
        current_dir.join(directory)
    };
    
    // Create target directory if it doesn't exist
    if directory != "." && !Path::new(directory).exists() {
        create_dir(directory)
            .map_err(|e| format!("Cannot create directory '{}': {}", directory, e))?;
    }
    
    // Change to target directory
    env::set_current_dir(directory)
        .map_err(|e| format!("Cannot change to directory '{}': {}", directory, e))?;
    
    let git_dir = if bare { "." } else { ".git" };
    
    // Check if repository already exists
    if Path::new(git_dir).join("HEAD").exists() {
        return Err(format!("Reinitialization of existing Git repository in {}/", 
                          target_path.display()));
    }
    
    // Create git directory structure
    if !bare {
        create_dir(".git")
            .map_err(|e| format!("Cannot create .git directory: {}", e))?;
    }
    
    // Create objects directory for storing git objects
    create_dir(&format!("{}/objects", git_dir))
        .map_err(|e| format!("Cannot create objects directory: {}", e))?;
    
    // Create refs directory for references
    create_dir(&format!("{}/refs", git_dir))
        .map_err(|e| format!("Cannot create refs directory: {}", e))?;
    
    // Create heads directory for branch references
    create_dir(&format!("{}/refs/heads", git_dir))
        .map_err(|e| format!("Cannot create refs/heads directory: {}", e))?;
    
    // Create tags directory for tag references
    create_dir(&format!("{}/refs/tags", git_dir))
        .map_err(|e| format!("Cannot create refs/tags directory: {}", e))?;
    
    // Create HEAD file pointing to master branch
    write_file(&format!("{}/HEAD", git_dir), "ref: refs/heads/master\n")
        .map_err(|e| format!("Cannot create HEAD file: {}", e))?;
    
    // Create basic configuration file
    let config_content = if bare {
        "[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = true\n"
    } else {
        "[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n"
    };
    
    write_file(&format!("{}/config", git_dir), config_content)
        .map_err(|e| format!("Cannot create config file: {}", e))?;
    
    // Create repository description file
    write_file(&format!("{}/description", git_dir), 
               "Unnamed repository; edit this file 'description' to name the repository.\n")
        .map_err(|e| format!("Cannot create description file: {}", e))?;
    
    Ok(target_path.display().to_string())
}


fn print_help() {
    println!("usage: rakke init [<options>] [<directory>]");
    println!();
    println!("    --bare                create a bare repository");
    println!("    -h, --help            show help");
}

// Helper function to create directory
fn create_dir(path: &str) -> io::Result<()> {
    fs::create_dir(path)
}

// Helper function to write file
fn write_file(path: &str, content: &str) -> io::Result<()> {
    fs::write(path, content)
}