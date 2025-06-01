use std::fs;
use std::path::Path;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::io::Write;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use byteorder::{BigEndian, WriteBytesExt};

pub fn execute(args: Vec<String>) {
    // Check if user provided any files to add
    if args.len() < 2 {
        eprintln!("Nothing specified, nothing added.");
        eprintln!("hint: Maybe you wanted to say 'rakke add .'?");
        std::process::exit(1);
    }
    
    // Extract file paths from command line (skip "add" command itself)
    let file_paths: Vec<String> = args[1..].to_vec();
    
    // Verify we are inside a git repository
    if !Path::new(".git").exists() {
        eprintln!("fatal: not a git repository (or any of the parent directories): .git");
        std::process::exit(1);
    }
    
    // Process each file or directory argument
    for path in file_paths {
        if let Err(e) = add_path(&path) {
            eprintln!("fatal: {}", e);
            std::process::exit(1);
        }
    }
}

fn add_path(path: &str) -> Result<(), String> {
    let path_obj = Path::new(path);
    
    // Check if the specified path exists
    if !path_obj.exists() {
        return Err(format!("pathspec '{}' did not match any files", path));
    }
    
    // Load existing index from .git/index file
    let mut index = load_index()?;
    
    if path_obj.is_file() {
        // Add single file to the index
        add_file_to_index(&mut index, path)?;
    } else if path_obj.is_dir() {
        // Add entire directory recursively to the index
        add_directory_to_index(&mut index, path)?;
    }
    
    // Save the updated index back to .git/index file
    save_index(&index)?;
    
    Ok(())
}

fn add_file_to_index(index: &mut HashMap<String, IndexEntry>, file_path: &str) -> Result<(), String> {
    // Read the entire file content into memory
    let content = fs::read(file_path)
        .map_err(|e| format!("Cannot read file '{}': {}", file_path, e))?;
    
    // Create git blob object and get its SHA-1 hash
    let blob_hash = create_blob_object(&content)?;
    
    // Get file system metadata (size, permissions, modification time)
    let metadata = fs::metadata(file_path)
        .map_err(|e| format!("Cannot get metadata for '{}': {}", file_path, e))?;
    
    // Create index entry with file information
    let entry = IndexEntry {
        hash: blob_hash,
        mode: get_file_mode(&metadata),
        size: content.len() as u32,
        mtime: get_mtime(&metadata),
    };
    
    // Insert or update the file in the index
    index.insert(file_path.to_string(), entry);
    
    Ok(())
}

fn add_directory_to_index(index: &mut HashMap<String, IndexEntry>, dir_path: &str) -> Result<(), String> {
    // Collect all files in directory recursively
    let mut files_to_add = Vec::new();
    collect_files(Path::new(dir_path), &mut files_to_add)?;
    
    // Add each collected file to the index
    for file_path in files_to_add {
        // Skip .git directory and its contents
        if file_path.starts_with(".git/") || file_path == ".git" {
            continue;
        }
        
        add_file_to_index(index, &file_path)?;
    }
    
    Ok(())
}

fn collect_files(dir: &Path, files: &mut Vec<String>) -> Result<(), String> {
    // Read directory entries
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Cannot read directory '{}': {}", dir.display(), e))?;
    
    // Process each entry in the directory
    for entry in entries {
        let entry = entry
            .map_err(|e| format!("Cannot read directory entry: {}", e))?;
        
        let path = entry.path();
        let path_str = path.to_str()
            .ok_or_else(|| "Invalid UTF-8 in file path".to_string())?;
        
        if path.is_file() {
            // Add regular file to the list
            files.push(path_str.to_string());
        } else if path.is_dir() {
            // Recursively process subdirectory
            collect_files(&path, files)?;
        }
    }
    
    Ok(())
}

fn create_blob_object(content: &[u8]) -> Result<String, String> {
    // Create git blob object format: "blob <size>\0<content>"
    let header = format!("blob {}\0", content.len());
    let mut object_content = header.into_bytes();
    object_content.extend_from_slice(content);
    
    // Calculate SHA-1 hash of the complete object
    let hash = sha1_hash(&object_content);
    
    // Compress object content using zlib
    let compressed = compress_zlib(&object_content)?;
    
    // Create object file path: .git/objects/xx/yyyyyyy...
    let (dir_name, file_name) = hash.split_at(2);
    let object_dir = format!(".git/objects/{}", dir_name);
    let object_path = format!("{}/{}", object_dir, file_name);
    
    // Create object directory if it doesn't exist
    if !Path::new(&object_dir).exists() {
        fs::create_dir_all(&object_dir)
            .map_err(|e| format!("Cannot create object directory: {}", e))?;
    }
    
    // Write compressed object to file (only if it doesn't already exist)
    if !Path::new(&object_path).exists() {
        fs::write(&object_path, compressed)
            .map_err(|e| format!("Cannot write object file: {}", e))?;
    }
    
    Ok(hash)
}

fn load_index() -> Result<HashMap<String, IndexEntry>, String> {
    let index_path = ".git/index";
    
    // Return empty index if file doesn't exist yet
    if !Path::new(index_path).exists() {
        return Ok(HashMap::new());
    }
    
    // Read existing index file
    let content = fs::read(index_path)
        .map_err(|e| format!("Cannot read index file: {}", e))?;
    
    // Parse index file format
    parse_index(&content)
}

fn save_index(index: &HashMap<String, IndexEntry>) -> Result<(), String> {
    let index_path = ".git/index";
    
    // Serialize index to git index format
    let content = serialize_index(index)?;
    
    // Write serialized index to file
    fs::write(index_path, content)
        .map_err(|e| format!("Cannot write index file: {}", e))?;
    
    Ok(())
}

fn parse_index(_content: &[u8]) -> Result<HashMap<String, IndexEntry>, String> {
    // TODO: Implement proper git index file parsing
    // For now, return empty index (existing files will be re-added)
    Ok(HashMap::new())
}

fn serialize_index(index: &HashMap<String, IndexEntry>) -> Result<Vec<u8>, String> {
    let mut content = Vec::new();
    
    // Write git index file signature "DIRC" (DIRtory Cache)
    content.extend_from_slice(b"DIRC");
    
    // Write index format version (version 2)
    content.write_u32::<BigEndian>(2)
        .map_err(|e| format!("Cannot write version: {}", e))?;
    
    // Write total number of index entries
    content.write_u32::<BigEndian>(index.len() as u32)
        .map_err(|e| format!("Cannot write entry count: {}", e))?;
    
    // Sort index entries by path for consistent output
    let mut entries: Vec<_> = index.iter().collect();
    entries.sort_by_key(|(path, _)| *path);
    
    // Write each index entry
    for (path, entry) in entries {
        write_index_entry(&mut content, path, entry)?;
    }
    
    // Calculate and append SHA-1 checksum of entire index
    let checksum = sha1_hash(&content);
    let checksum_bytes = hex_to_bytes(&checksum)?;
    content.extend_from_slice(&checksum_bytes);
    
    Ok(content)
}

fn write_index_entry(content: &mut Vec<u8>, path: &str, entry: &IndexEntry) -> Result<(), String> {
    // Write creation time (set to modification time for simplicity)
    content.write_u32::<BigEndian>(entry.mtime)
        .map_err(|e| format!("Cannot write ctime: {}", e))?;
    content.write_u32::<BigEndian>(0) // nanoseconds
        .map_err(|e| format!("Cannot write ctime_ns: {}", e))?;
    
    // Write modification time
    content.write_u32::<BigEndian>(entry.mtime)
        .map_err(|e| format!("Cannot write mtime: {}", e))?;
    content.write_u32::<BigEndian>(0) // nanoseconds
        .map_err(|e| format!("Cannot write mtime_ns: {}", e))?;
    
    // Write device and inode (set to 0 for cross-platform compatibility)
    content.write_u32::<BigEndian>(0) // device
        .map_err(|e| format!("Cannot write device: {}", e))?;
    content.write_u32::<BigEndian>(0) // inode
        .map_err(|e| format!("Cannot write inode: {}", e))?;
    
    // Write file mode (permissions and file type)
    content.write_u32::<BigEndian>(entry.mode)
        .map_err(|e| format!("Cannot write mode: {}", e))?;
    
    // Write user and group IDs (set to 0 for simplicity)
    content.write_u32::<BigEndian>(0) // uid
        .map_err(|e| format!("Cannot write uid: {}", e))?;
    content.write_u32::<BigEndian>(0) // gid
        .map_err(|e| format!("Cannot write gid: {}", e))?;
    
    // Write file size
    content.write_u32::<BigEndian>(entry.size)
        .map_err(|e| format!("Cannot write size: {}", e))?;
    
    // Write SHA-1 hash (20 bytes)
    let hash_bytes = hex_to_bytes(&entry.hash)?;
    if hash_bytes.len() != 20 {
        return Err("Invalid SHA-1 hash length".to_string());
    }
    content.extend_from_slice(&hash_bytes);
    
    // Write flags (assume no conflicts, stage 0)
    let path_len = std::cmp::min(path.len(), 0xfff); // max 12 bits for path length
    content.write_u16::<BigEndian>(path_len as u16)
        .map_err(|e| format!("Cannot write flags: {}", e))?;
    
    // Write file path with null terminator
    content.extend_from_slice(path.as_bytes());
    content.push(0); // null terminator
    
    // Pad to 8-byte boundary for proper alignment
    while content.len() % 8 != 0 {
        content.push(0);
    }
    
    Ok(())
}

// Index entry structure representing a single file in the git index
#[derive(Debug, Clone)]
struct IndexEntry {
    hash: String,    // SHA-1 hash of the file content
    mode: u32,       // File permissions and type
    size: u32,       // File size in bytes
    mtime: u32,      // Last modification time
}

// Calculate SHA-1 hash using a simple implementation
fn sha1_hash(data: &[u8]) -> String {
    // Simple SHA-1 implementation for git objects
    // NOTE: This is a basic implementation, production code should use a crypto library
    
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;
    
    // Pre-processing: adding padding bits
    let mut padded = data.to_vec();
    let original_len = data.len();
    
    // Append '1' bit (0x80 byte)
    padded.push(0x80);
    
    // Append zeros until length ≡ 448 (mod 512)
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    
    // Append original length as 64-bit big-endian
    let bit_len = (original_len as u64) * 8;
    padded.extend_from_slice(&bit_len.to_be_bytes());
    
    // Process message in 512-bit chunks
    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        
        // Break chunk into sixteen 32-bit words
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1], 
                chunk[i * 4 + 2],
                chunk[i * 4 + 3]
            ]);
        }
        
        // Extend words
        for i in 16..80 {
            w[i] = (w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16]).rotate_left(1);
        }
        
        // Initialize hash values for this chunk
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        
        // Main loop
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                60..=79 => (b ^ c ^ d, 0xCA62C1D6),
                _ => unreachable!(),
            };
            
            let temp = a.rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        
        // Add this chunk's hash to result
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }
    
    // Format final hash as hexadecimal string
    format!("{:08x}{:08x}{:08x}{:08x}{:08x}", h0, h1, h2, h3, h4)
}

// Compress data using zlib compression
fn compress_zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)
        .map_err(|e| format!("Compression error: {}", e))?;
    
    encoder.finish()
        .map_err(|e| format!("Compression finish error: {}", e))
}

// Get file mode (permissions) from metadata
fn get_file_mode(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        let mode = metadata.permissions().mode();
        if mode & 0o111 != 0 {
            0o100755 // Executable file
        } else {
            0o100644 // Regular file
        }
    }
    #[cfg(not(unix))]
    {
        0o100644 // Default to regular file on non-Unix systems
    }
}

// Get modification time from metadata as Unix timestamp
fn get_mtime(metadata: &fs::Metadata) -> u32 {
    use std::time::SystemTime;
    
    metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32
}

// Convert hexadecimal string to byte array
fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    
    // Process hex string in pairs of characters
    for chunk in hex.as_bytes().chunks(2) {
        let hex_str = std::str::from_utf8(chunk)
            .map_err(|_| "Invalid hex string")?;
        let byte = u8::from_str_radix(hex_str, 16)
            .map_err(|_| "Invalid hex digit")?;
        bytes.push(byte);
    }
    
    Ok(bytes)
}