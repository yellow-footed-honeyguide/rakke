use std::io::{Read, Cursor};   // For reading bytes and cursor functionality
use std::error::Error;         // For error handling traits
use flate2::read::ZlibDecoder; // For zlib decompression

// Define Git object types with derived traits for debugging, cloning and comparison
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectType {
    Commit,   // Git commit object
    Tree,     // Git tree object (directory structure)
    Blob,     // Git blob object (file content)
    Tag,      // Git tag object
    Unknown,  // For unrecognized object types
}


#[derive(Debug, Clone)]  // Git object structure with debug and clone capabilities
pub struct GitObject {
    pub hash: String,    // SHA-1 hash of the object
    pub object_type: ObjectType, // Type of the Git object
    pub size: usize,     // Size of the object content
    pub data: Vec<u8>,   // Raw content data of the object
}

impl GitObject {
    // Creates a GitObject from raw compressed data
    pub fn from_raw_data(hash: &str, raw_data: &[u8]) -> Result<Self, Box<dyn Error>> {
        // Create a zlib decoder with a cursor over the raw data
        let mut decoder = ZlibDecoder::new(Cursor::new(raw_data));
        let mut decompressed_data = Vec::new();
        // Decompress the entire data into the vector
        decoder.read_to_end(&mut decompressed_data)?;
        
        // Parse the decompressed data into a GitObject
        Self::parse_object_data(hash, &decompressed_data)
    }

    // Parses the Git object data format: "type size\0content"
    fn parse_object_data(hash: &str, data: &[u8]) -> Result<Self, Box<dyn Error>> {
        // Find the null byte separator between header and content
        let null_pos = data.iter().position(|&b| b == 0).ok_or("Invalid object format")?;
        
        // Convert header part (before null byte) to UTF-8 string
        let header = std::str::from_utf8(&data[0..null_pos])?;
        // Split header into type and size components
        let parts: Vec<&str> = header.split(' ').collect();
        
        // Validate header format - must have exactly 2 parts
        if parts.len() != 2 {
            return Err("Invalid object header format".into());
        }
        
        // Match the object type string to our enum
        let object_type = match parts[0] {
            "commit" => ObjectType::Commit,
            "tree" => ObjectType::Tree,
            "blob" => ObjectType::Blob,
            "tag" => ObjectType::Tag,
            _ => ObjectType::Unknown,
        };
        
        // Parse the size component as usize
        let size = parts[1].parse::<usize>()?;
        // Extract content (everything after null byte)
        let content = data[null_pos + 1..].to_vec();
        
        // Construct and return the GitObject
        Ok(GitObject {
            hash: hash.to_string(), // Store the provided hash
            object_type,             // Determined object type
            size,                    // Parsed content size
            data: content,           // Actual object content
        })
    }

    // Creates a GitObject from already decompressed data
    pub fn from_decompressed_data(hash: &str, data: &[u8]) -> Result<Self, Box<dyn Error>> {
        // Simply parse the data (assumes it's already decompressed)
        Self::parse_object_data(hash, data)
    }
    
    // Extracts the object type from a header byte slice
    pub fn extract_type_from_header(header: &[u8]) -> Result<ObjectType, Box<dyn Error>> {
        // Convert header bytes to UTF-8 string
        let header_str = std::str::from_utf8(header)?;
        // Split header into components
        let parts: Vec<&str> = header_str.split(' ').collect();
        
        // Validate we have at least one part (the type)
        if parts.len() < 1 {
            return Err("Invalid object header format".into());
        }
        
        // Match the type string to our enum
        let object_type = match parts[0] {
            "commit" => ObjectType::Commit,
            "tree" => ObjectType::Tree,
            "blob" => ObjectType::Blob,
            "tag" => ObjectType::Tag,
            _ => ObjectType::Unknown,
        };
        
        Ok(object_type)
    }
}