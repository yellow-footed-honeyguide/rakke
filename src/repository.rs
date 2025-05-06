// Import necessary standard library components
use std::path::{Path, PathBuf};  // For path manipulation
use std::fs;                     // For filesystem operations
use std::error::Error;           // For error handling
use std::collections::{HashSet, HashMap};  // For data structures

// Import crate-local modules
use crate::objects::{GitObject, ObjectType};  // Git object types
use crate::pack::PackFile;                    // Pack file handling

// Repository struct representing a Git repository
pub struct Repository {
    git_dir: PathBuf,  // Path to the .git directory
    objects_cache: HashMap<String, GitObject>,  // Cache for loaded Git objects
}

impl Repository {
    // Creates a new Repository instance by finding the .git directory
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let git_dir = find_git_dir(path.as_ref())?;  // Find .git directory
        Ok(Repository { 
            git_dir, 
            objects_cache: HashMap::new(),  // Initialize empty cache
        })
    }

    // Counts all commit objects in the repository
    pub fn count_all_commits(&self) -> Result<usize, Box<dyn Error>> {
        let objects = self.get_all_objects()?;  // Get all objects
        
        // Filter and count only commit objects
        let commit_count = objects.iter()
            .filter(|obj| obj.object_type == ObjectType::Commit)
            .count();
        
        Ok(commit_count)
    }

    // Retrieves all Git objects (both loose and packed)
    fn get_all_objects(&self) -> Result<Vec<GitObject>, Box<dyn Error>> {
        let mut objects = Vec::new();  // Initialize collection
        
        self.add_loose_objects(&mut objects)?;  // Add loose objects
        self.add_packed_objects(&mut objects)?;  // Add packed objects
        
        // Deduplicate objects by their hash
        let mut unique_hashes = HashSet::new();  // Track seen hashes
        let mut unique_objects = Vec::new();    // Store unique objects
        
        for obj in objects {
            if unique_hashes.insert(obj.hash.clone()) {  // Check if new hash
                unique_objects.push(obj);  // Add if unique
            }
        }
        
        Ok(unique_objects)
    }

    // Adds loose objects from objects directory
    fn add_loose_objects(&self, objects: &mut Vec<GitObject>) -> Result<(), Box<dyn Error>> {
        let objects_dir = self.git_dir.join("objects");  // Path to objects dir
        
        if !objects_dir.exists() {  // Check if objects directory exists
            return Err("Objects directory not found".into());
        }
        
        // Process each entry in objects directory
        for entry in fs::read_dir(&objects_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {  // Only process directories
                let dir_name = path.file_name().unwrap().to_string_lossy();
                if dir_name == "info" || dir_name == "pack" {  // Skip special dirs
                    continue;
                }
                
                let prefix = dir_name.to_string();  // First 2 chars of hash
                
                // Process each file in the hash prefix directory
                for file_entry in fs::read_dir(path)? {
                    let file_entry = file_entry?;
                    let file_path = file_entry.path();
                    
                    if file_path.is_file() {  // Only process files
                        let suffix = file_path.file_name().unwrap().to_string_lossy();
                        let hash = format!("{}{}", prefix, suffix);  // Full hash
                        
                        // Try to load the object and add to collection
                        if let Ok(obj) = self.load_loose_object(&hash) {
                            objects.push(obj);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }

    // Loads a single loose object by its hash
    fn load_loose_object(&self, hash: &str) -> Result<GitObject, Box<dyn Error>> {
        if hash.len() < 2 {  // Validate hash length
            return Err("Hash too short".into());
        }
        
        let prefix = &hash[0..2];  // First 2 chars (directory name)
        let suffix = &hash[2..];    // Remaining chars (filename)
        let object_path = self.git_dir.join("objects").join(prefix).join(suffix);
        
        if !object_path.exists() {  // Check if object exists
            return Err(format!("Object not found: {}", hash).into());
        }
        
        let raw_data = fs::read(object_path)?;  // Read raw object data
        
        GitObject::from_raw_data(hash, &raw_data)  // Parse into GitObject
    }

    // Adds objects from pack files
    fn add_packed_objects(&self, objects: &mut Vec<GitObject>) -> Result<(), Box<dyn Error>> {
        let pack_dir = self.git_dir.join("objects").join("pack");  // Pack dir path
        
        if !pack_dir.exists() {  // Skip if no pack directory
            return Ok(());
        }
        
        // Process each entry in pack directory
        for entry in fs::read_dir(pack_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            // Look for .pack files
            if path.is_file() && path.extension().map_or(false, |ext| ext == "pack") {
                let pack_file = PackFile::new(path)?;  // Create PackFile instance
                let pack_objects = pack_file.extract_objects()?;  // Extract objects
                
                objects.extend(pack_objects);  // Add to collection
            }
        }
        
        Ok(())
    }
}

// Finds the .git directory by walking up from start_path
fn find_git_dir(start_path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut current_path = start_path.to_path_buf();  // Start at given path
    
    loop {
        let git_dir = current_path.join(".git");  // Check for .git
        
        if git_dir.exists() && git_dir.is_dir() {  // Found directory
            return Ok(git_dir);
        }
        
        // Handle gitdir files (for submodules)
        if git_dir.exists() && git_dir.is_file() {
            let content = fs::read_to_string(git_dir)?;  // Read file
            if let Some(real_path) = content.strip_prefix("gitdir: ") {  // Parse content
                let real_git_dir = Path::new(real_path.trim());
                if real_git_dir.is_absolute() {  // Handle absolute path
                    return Ok(real_git_dir.to_path_buf());
                } else {  // Handle relative path
                    return Ok(current_path.join(real_git_dir));
                }
            }
        }
        
        if !current_path.pop() {  // Move to parent directory
            return Err(".git directory not found".into());  // Reached root
        }
    }
}