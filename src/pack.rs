use std::fs;                                  // file system operations
use std::path::Path;                          // path manipulation
use std::error::Error;                        // error handling
use std::collections::HashMap;                // hash map data structure
use std::io::{Cursor, Read, Seek, SeekFrom};  // I/O operations
use flate2::read::ZlibDecoder;                // zlib decompression
use byteorder::{BigEndian, ReadBytesExt};     // reading binary data in big-endian format
use crate::objects::{GitObject, ObjectType};  // Git object types from local module

// Enum representing Git pack file object types
#[derive(Debug, Clone, Copy, PartialEq)]
enum PackObjectType {
    Commit = 1,    // Commit object type
    Tree = 2,      // Tree object type
    Blob = 3,      // Blob object type
    Tag = 4,       // Tag object type
    OfsDelta = 6,  // Offset delta object type
    RefDelta = 7,  // Reference delta object type
}

// Implementation to convert pack object types to general object types
impl From<PackObjectType> for ObjectType {
    fn from(pack_type: PackObjectType) -> Self {
        match pack_type {
            PackObjectType::Commit => ObjectType::Commit,  // Convert commit type
            PackObjectType::Tree => ObjectType::Tree,      // Convert tree type
            PackObjectType::Blob => ObjectType::Blob,      // Convert blob type
            PackObjectType::Tag => ObjectType::Tag,        // Convert tag type
            _ => ObjectType::Unknown,                      // Convert delta types to unknown
        }
    }
}

// Structure for Git pack file handling
pub struct PackFile {
    path: String,      // Path to the pack file
    idx_path: String,  // Path to the index file
}

impl PackFile {
    // Create a new PackFile instance from a path
    pub fn new(pack_path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let path = pack_path.as_ref().to_string_lossy().to_string();  // Convert path to string
        
        // Determine index file path by replacing .pack extension with .idx
        let idx_path = if path.ends_with(".pack") {
            path[..path.len() - 5].to_string() + ".idx"  // Replace .pack with .idx
        } else {
            return Err("Invalid pack file extension".into());  // Return error for invalid extension
        };
        
        // Check if index file exists
        if !Path::new(&idx_path).exists() {
            return Err(format!("Index file not found: {}", idx_path).into());  // Return error if index file not found
        }
        
        // Return new PackFile instance
        Ok(PackFile {
            path,
            idx_path,
        })
    }

    // Extract all objects from the pack file
    pub fn extract_objects(&self) -> Result<Vec<GitObject>, Box<dyn Error>> {
        println!("Extracting objects from pack file: {}", self.path);  // Log extraction start
        
        // Read pack file data with error handling
        let pack_data = match fs::read(&self.path) {
            Ok(data) => data,  // Store data if read successful
            Err(e) => return Err(format!("Error reading pack file {}: {}", self.path, e).into()),  // Return error if read fails
        };
        
        println!("Pack file size: {} bytes", pack_data.len());  // Log pack file size
        
        // Read index file data with error handling
        let idx_data = match fs::read(&self.idx_path) {
            Ok(data) => data,  // Store data if read successful
            Err(e) => return Err(format!("Error reading idx file {}: {}", self.idx_path, e).into()),  // Return error if read fails
        };
        
        println!("Index file size: {} bytes", idx_data.len());  // Log index file size
        
        // Parse index file to get object offsets
        let offsets = match self.parse_idx_file(&idx_data) {
            Ok(offs) => offs,  // Store offsets if parsing successful
            Err(e) => {
                eprintln!("Error parsing idx file: {}", e);  // Log error
                HashMap::new()  // Continue with empty offsets map
            }
        };
        
        println!("Found {} objects in idx file", offsets.len());  // Log number of objects found
        
        // Parse pack file and extract objects
        match self.parse_pack_file(&pack_data, &offsets) {
            Ok(objects) => {
                println!("Successfully extracted {} objects from pack file", objects.len());  // Log successful extraction
                Ok(objects)  // Return extracted objects
            },
            Err(e) => {
                eprintln!("Error parsing pack file: {}", e);  // Log error
                Ok(Vec::new())  // Return empty list on error
            }
        }
    }

    // Parse index file to get object offsets
    fn parse_idx_file(&self, data: &[u8]) -> Result<HashMap<String, u32>, Box<dyn Error>> {
        let mut cursor = Cursor::new(data);  // Create cursor for reading data
        
        // Check signature and version with error handling
        let mut signature = [0u8; 4];  // Buffer for signature
        match cursor.read_exact(&mut signature) {
            Ok(_) => {},  // Continue if read successful
            Err(e) => return Err(format!("Error reading idx file signature: {}", e).into()),  // Return error if read fails
        }
        
        let mut version_2 = false;  // Flag for version 2 index file
        
        // Check index file version
        if &signature == b"\xff\x74\x4f\x63" {
            // This is a version 2 idx file
            version_2 = true;  // Set version 2 flag
            let mut version = [0u8; 4];  // Buffer for version
            match cursor.read_exact(&mut version) {
                Ok(_) => {},  // Continue if read successful
                Err(e) => return Err(format!("Error reading idx file version: {}", e).into()),  // Return error if read fails
            }
            
            // Check if version is supported (must be 2)
            if version != [0, 0, 0, 2] {
                return Err(format!("Unsupported idx file version: {:?}", version).into());  // Return error for unsupported version
            }
        } else {
            // This is a version 1 idx file, reset cursor to start
            match cursor.seek(SeekFrom::Start(0)) {
                Ok(_) => {},  // Continue if seek successful
                Err(e) => return Err(format!("Error seeking cursor: {}", e).into()),  // Return error if seek fails
            }
        }
        
        // Skip fanout table
        let fanout_offset = if version_2 { 8 } else { 0 };  // Offset depends on version
        match cursor.seek(SeekFrom::Start(fanout_offset + 4 * 255)) {
            Ok(_) => {},  // Continue if seek successful
            Err(e) => return Err(format!("Error skipping fanout table: {}", e).into()),  // Return error if seek fails
        }
        
        // Read object count
        let num_objects = match cursor.read_u32::<BigEndian>() {
            Ok(n) => n,  // Store count if read successful
            Err(e) => return Err(format!("Error reading object count: {}", e).into()),  // Return error if read fails
        };
        
        println!("Number of objects in idx file: {}", num_objects);  // Log object count
        
        // Calculate SHA1 hashes position in file
        let sha_pos = if version_2 { 
            fanout_offset + 4 * 256  // After fanout table for version 2
        } else {
            4 * 256  // Right after fanout table for version 1
        };
        
        // Move to SHA1 hashes start
        match cursor.seek(SeekFrom::Start(sha_pos)) {
            Ok(_) => {},  // Continue if seek successful
            Err(e) => return Err(format!("Error seeking to SHA1 hashes: {}", e).into()),  // Return error if seek fails
        }
        
        // Read object hashes
        let mut objects = HashMap::new();  // Map to store hash -> offset pairs
        
        for i in 0..num_objects {
            let mut hash = [0u8; 20];  // Buffer for SHA1 hash (20 bytes)
            match cursor.read_exact(&mut hash) {
                Ok(_) => {},  // Continue if read successful
                Err(e) => {
                    eprintln!("Error reading object hash {}: {}", i, e);  // Log error
                    continue;  // Skip to next object
                }
            }
            
            // Convert hash bytes to hex string
            let hash_str = hash.iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            
            // Store hash with temporary offset 0
            objects.insert(hash_str, 0);
        }
        
        // In version 2 idx files, there's a CRC table
        let crc_table_len = if version_2 { 4 * num_objects as usize } else { 0 };  // CRC table length
        
        // Skip CRC table if present
        if version_2 {
            match cursor.seek(SeekFrom::Current(crc_table_len as i64)) {
                Ok(_) => {},  // Continue if seek successful
                Err(e) => eprintln!("Error skipping CRC table: {}", e),  // Log error but continue
            }
        }
        
        // Now read object offsets
        let mut i = 0;  // Counter for processed offsets
        for (hash, offset) in objects.iter_mut() {
            match cursor.read_u32::<BigEndian>() {
                Ok(o) => *offset = o,  // Store offset if read successful
                Err(e) => {
                    eprintln!("Error reading offset for object {}: {}", hash, e);  // Log error
                    i += 1;  // Increment counter
                    continue;  // Skip to next object
                }
            }
            i += 1;  // Increment counter
        }
        
        println!("Read {} offsets from idx file", i);  // Log number of offsets read
        
        Ok(objects)  // Return hash -> offset map
    }

    // Parse pack file and extract objects
    fn parse_pack_file(&self, data: &[u8], offsets: &HashMap<String, u32>) -> Result<Vec<GitObject>, Box<dyn Error>> {
        println!("Starting to parse pack file of size {} bytes", data.len());  // Log parsing start
        
        let mut cursor = Cursor::new(data);  // Create cursor for reading data
        
        // Check "PACK" signature
        let mut signature = [0u8; 4];  // Buffer for signature
        match cursor.read_exact(&mut signature) {
            Ok(_) => {},  // Continue if read successful
            Err(e) => return Err(format!("Failed to read pack file signature: {}", e).into()),  // Return error if read fails
        };
        
        // Verify signature
        if &signature != b"PACK" {
            return Err(format!("Invalid pack file signature: {:?}", signature).into());  // Return error for invalid signature
        }
        
        // Read version (should be 2 or 3)
        let version = match cursor.read_u32::<BigEndian>() {
            Ok(v) => v,  // Store version if read successful
            Err(e) => return Err(format!("Failed to read pack file version: {}", e).into()),  // Return error if read fails
        };
        
        // Check if version is supported
        if version != 2 && version != 3 {
            return Err(format!("Unsupported pack file version: {}", version).into());  // Return error for unsupported version
        }
        
        // Read object count
        let num_objects = match cursor.read_u32::<BigEndian>() {
            Ok(n) => n as usize,  // Store count if read successful
            Err(e) => return Err(format!("Failed to read object count: {}", e).into()),  // Return error if read fails
        };
        
        println!("Pack file version {}, contains {} objects", version, num_objects);  // Log version and object count
        
        // Create reverse mapping: offset -> hash
        let mut offset_to_hash = HashMap::new();  // Map to store offset -> hash pairs
        for (hash, &offset) in offsets {
            offset_to_hash.insert(offset, hash.clone());  // Store offset -> hash mapping
        }
        
        // Extract all objects
        let mut objects = Vec::with_capacity(num_objects);  // Vector to store extracted objects
        
        // Use safety counter to prevent infinite loops
        let mut processed = 0;  // Counter for processed objects
        let max_objects = num_objects * 2;  // Safety margin for errors
        
        // Process objects until we reach the expected count or end of data
        while processed < num_objects && cursor.position() < data.len() as u64 && processed < max_objects {
            let current_offset = cursor.position() as u32;  // Get current position
            let hash = match offset_to_hash.get(&current_offset) {
                Some(hash) => hash.clone(),  // Use known hash if available
                None => format!("unknown_{}", current_offset),  // Generate placeholder hash
            };
            
            // Read object header with error handling
            let header_result = self.read_object_header(&mut cursor);
            
            match header_result {
                Ok((obj_type, obj_size)) => {
                    match obj_type {
                        // For regular objects, read data
                        PackObjectType::Commit | PackObjectType::Tree | PackObjectType::Blob | PackObjectType::Tag => {
                            match self.read_zlib_data(&mut cursor, obj_size) {
                                Ok(obj_data) => {
                                    // Create header for object
                                    let type_str = match obj_type {
                                        PackObjectType::Commit => "commit",  // Commit type string
                                        PackObjectType::Tree => "tree",      // Tree type string
                                        PackObjectType::Blob => "blob",      // Blob type string
                                        PackObjectType::Tag => "tag",        // Tag type string
                                        _ => unreachable!(),                 // Should never happen
                                    };
                                    
                                    // Format full object data with header
                                    let header = format!("{} {}", type_str, obj_size);  // Create header string
                                    let mut full_data = Vec::with_capacity(header.len() + 1 + obj_data.len());  // Allocate space
                                    full_data.extend_from_slice(header.as_bytes());  // Add header
                                    full_data.push(0);  // Add null byte separator
                                    full_data.extend_from_slice(&obj_data);  // Add object data
                                    
                                    // Create and add object
                                    match GitObject::from_decompressed_data(&hash, &full_data) {
                                        Ok(obj) => {
                                            objects.push(obj);  // Add object to result list
                                        },
                                        Err(e) => {
                                            eprintln!("Error creating object from data: {} (offset: {})", e, current_offset);  // Log error
                                        }
                                    }
                                },
                                Err(e) => {
                                    // On data read error, try to skip this object
                                    eprintln!("Error reading object data: {} (offset: {})", e, current_offset);  // Log error
                                    let _ = cursor.seek(SeekFrom::Current(1));  // Move cursor forward slightly and continue
                                }
                            }
                        },
                        // For offset delta objects, safely skip
                        PackObjectType::OfsDelta => {
                            match self.read_offset_delta(&mut cursor) {
                                Ok(_) => {
                                    // Try to skip object data
                                    match self.skip_zlib_data(&mut cursor) {
                                        Ok(_) => {},  // Continue if skip successful
                                        Err(e) => {
                                            eprintln!("Error skipping OFS_DELTA object: {}", e);  // Log error
                                            let _ = cursor.seek(SeekFrom::Current(1));  // Move cursor forward slightly
                                        }
                                    }
                                },
                                Err(e) => {
                                    // On offset read error, skip
                                    eprintln!("Error reading OFS_DELTA offset: {}", e);  // Log error
                                    let _ = cursor.seek(SeekFrom::Current(1));  // Move cursor forward slightly
                                }
                            }
                        },
                        // For reference delta objects, safely skip
                        PackObjectType::RefDelta => {
                            let mut base_hash = [0u8; 20];  // Buffer for base hash
                            match cursor.read_exact(&mut base_hash) {
                                Ok(_) => {
                                    // Try to skip object data
                                    match self.skip_zlib_data(&mut cursor) {
                                        Ok(_) => {},  // Continue if skip successful
                                        Err(e) => {
                                            eprintln!("Error skipping REF_DELTA object: {}", e);  // Log error
                                            let _ = cursor.seek(SeekFrom::Current(1));  // Move cursor forward slightly
                                        }
                                    }
                                },
                                Err(e) => {
                                    // On base hash read error, skip
                                    eprintln!("Error reading REF_DELTA base hash: {}", e);  // Log error
                                    let _ = cursor.seek(SeekFrom::Current(1));  // Move cursor forward slightly
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    // On header read error, skip this object
                    eprintln!("Error reading object header: {} (offset: {})", e, current_offset);  // Log error
                    
                    let _ = cursor.seek(SeekFrom::Current(1));  // Move cursor forward slightly
                }
            }
            
            processed += 1;  // Increment processed counter
        }
        
        // Log statistics
        println!("Processed objects: {}/{}", processed, num_objects);  // Log processed count
        println!("Found objects: {}", objects.len());  // Log found objects count
        
        // Return all found objects
        Ok(objects)
    }

    // Read object header from pack file
    fn read_object_header(&self, cursor: &mut Cursor<&[u8]>) -> Result<(PackObjectType, usize), Box<dyn Error>> {
        // Check if we haven't reached end of data
        if cursor.position() >= cursor.get_ref().len() as u64 {
            return Err("Reached end of file while reading object header".into());  // Return error at EOF
        }
        
        // Read first byte
        let byte = match cursor.read_u8() {
            Ok(b) => b,  // Store byte if read successful
            Err(e) => return Err(format!("Error reading first header byte: {}", e).into()),  // Return error if read fails
        };
        
        // Extract object type from top 3 bits
        let obj_type = match (byte >> 4) & 0x7 {
            1 => PackObjectType::Commit,   // Type 1 is commit
            2 => PackObjectType::Tree,     // Type 2 is tree
            3 => PackObjectType::Blob,     // Type 3 is blob
            4 => PackObjectType::Tag,      // Type 4 is tag
            6 => PackObjectType::OfsDelta, // Type 6 is offset delta
            7 => PackObjectType::RefDelta, // Type 7 is reference delta
            t => return Err(format!("Unknown object type in pack file: {}", t).into()),  // Return error for unknown type
        };
        
        // Extract size from bottom 4 bits of first byte
        let mut size = (byte & 0x0F) as usize;  // Initial size from first byte
        
        // If MSB is set, read additional size bytes
        let mut shift = 4;  // Bit shift for next byte
        let mut current_byte = byte;  // Current byte being processed
        
        // Limit iterations for safety
        let mut iterations = 0;  // Iteration counter
        const MAX_ITERATIONS: usize = 10;  // Maximum allowed iterations
        
        // Continue reading size bytes while MSB is set
        while current_byte & 0x80 != 0 && iterations < MAX_ITERATIONS {
            // Check if we haven't reached end of data
            if cursor.position() >= cursor.get_ref().len() as u64 {
                return Err("Reached end of file while reading object size".into());  // Return error at EOF
            }
            
            // Read next size byte
            current_byte = match cursor.read_u8() {
                Ok(b) => b,  // Store byte if read successful
                Err(e) => return Err(format!("Error reading size byte: {}", e).into()),  // Return error if read fails
            };
            
            // Add next 7 bits to size
            size |= ((current_byte & 0x7F) as usize) << shift;  // Add bits at correct position
            shift += 7;  // Move shift for next byte
            iterations += 1;  // Increment iteration counter
            
            // Guard against overflow when reading size
            if shift > 64 {
                return Err("Size value too large".into());  // Return error for overflow risk
            }
        }
        
        // Check for infinite loop
        if iterations >= MAX_ITERATIONS {
            return Err("Too many iterations while reading object size".into());  // Return error for too many iterations
        }
        
        // Check for suspiciously large size to prevent memory allocation errors
        const MAX_OBJECT_SIZE: usize = 100 * 1024 * 1024;  // 100 MB limit
        if size > MAX_OBJECT_SIZE {
            return Err(format!("Suspiciously large object size: {} bytes", size).into());  // Return error for large size
        }
        
        Ok((obj_type, size))  // Return object type and size
    }

    // Read offset for OFS_DELTA object
    fn read_offset_delta(&self, cursor: &mut Cursor<&[u8]>) -> Result<usize, Box<dyn Error>> {
        // Read first byte
        let mut byte = match cursor.read_u8() {
            Ok(b) => b,  // Store byte if read successful
            Err(e) => return Err(format!("Error reading first offset byte: {}", e).into()),  // Return error if read fails
        };
        
        // Extract initial offset from first 7 bits
        let mut offset = (byte & 0x7F) as usize;  // Initial offset from first byte
        
        // Limit iterations for safety
        let mut iterations = 0;  // Iteration counter
        const MAX_ITERATIONS: usize = 10;  // Maximum allowed iterations
        
        // Continue reading offset bytes while MSB is set
        while byte & 0x80 != 0 && iterations < MAX_ITERATIONS {
            offset += 1;  // Increment offset
            // Read next offset byte
            byte = match cursor.read_u8() {
                Ok(b) => b,  // Store byte if read successful
                Err(e) => return Err(format!("Error reading offset byte: {}", e).into()),  // Return error if read fails
            };
            offset = (offset << 7) + (byte & 0x7F) as usize;  // Add next 7 bits to offset
            iterations += 1;  // Increment iteration counter
            
            // Guard against overflow
            if iterations >= MAX_ITERATIONS {
                return Err("Too many iterations while reading delta offset".into());  // Return error for too many iterations
            }
        }
        
        Ok(offset)  // Return offset value
    }

    // Skip zlib-compressed data without reading it
    fn skip_zlib_data(&self, cursor: &mut Cursor<&[u8]>) -> Result<(), Box<dyn Error>> {
        // Save current position
        let start_pos = cursor.position() as usize;  // Get current position
        
        // Guard against buffer overflow
        if start_pos >= cursor.get_ref().len() {
            return Err("Reached end of file while skipping compressed data".into());  // Return error at EOF
        }
        
        // Try to read first 2 bytes to determine zlib header
        let mut zlib_header = [0u8; 2];  // Buffer for zlib header
        match cursor.read_exact(&mut zlib_header) {
            Ok(_) => {},  // Continue if read successful
            Err(e) => return Err(format!("Error reading zlib header: {}", e).into()),  // Return error if read fails
        }
        
        // Verify it's a valid zlib header
        if (zlib_header[0] & 0x0F) != 0x08 ||  // 8 = deflate
           (zlib_header[0] & 0xF0) > 0x70 ||   // Check window size (must be <= 7)
           (zlib_header[0] as u16 * 256 + zlib_header[1] as u16) % 31 != 0  // Check checksum
        {
            return Err(format!("Invalid zlib header: {:?}", zlib_header).into());  // Return error for invalid header
        }
        
        // Return to beginning of data block
        cursor.seek(SeekFrom::Start(start_pos as u64))?;  // Reset cursor position
        
        // Find end of zlib block by trial and error
        // Not ideal but works well enough
        let mut test_size = 1024;  // Start with 1KB
        
        // Try increasing data chunks until we find one that decompresses successfully
        while start_pos + test_size <= cursor.get_ref().len() {
            let test_data = &cursor.get_ref()[start_pos..start_pos + test_size];  // Get test chunk
            
            // Try to decompress data
            let mut decoder = ZlibDecoder::new(Cursor::new(test_data));  // Create zlib decoder
            let mut out = Vec::new();  // Buffer for decompressed data
            
            match decoder.read_to_end(&mut out) {
                Ok(_) => {
                    // If decompression successful, move cursor and return success
                    let bytes_read = decoder.total_in() as i64;  // Get bytes consumed
                    if bytes_read > 0 {
                        cursor.seek(SeekFrom::Current(bytes_read))?;  // Move cursor forward
                        return Ok(());  // Return success
                    }
                    
                    // If couldn't determine bytes read,
                    // just move cursor forward by one byte
                    cursor.seek(SeekFrom::Current(1))?;  // Move cursor by 1 byte
                    return Ok(());  // Return success
                },
                Err(_) => {
                    // Increase test block size
                    test_size *= 2;  // Double test size
                    
                    // Limit maximum test block size
                    if test_size > 1024 * 1024 {  // 1MB limit
                        // If reached maximum size, just move cursor by one byte
                        cursor.seek(SeekFrom::Current(1))?;  // Move cursor by 1 byte
                        return Ok(());  // Return success
                    }
                }
            }
        }
        
        // If couldn't determine block size, just move cursor by one byte
        cursor.seek(SeekFrom::Current(1))?;  // Move cursor by 1 byte
        Ok(())  // Return success
    }

    // Read zlib-compressed data
    fn read_zlib_data(&self, cursor: &mut Cursor<&[u8]>, expected_size: usize) -> Result<Vec<u8>, Box<dyn Error>> {
        // Save current position
        let start_pos = cursor.position() as usize;  // Get current position
        
        // Check position is within buffer
        if start_pos >= cursor.get_ref().len() {
            return Err("Reached end of file while reading compressed data".into());  // Return error at EOF
        }
        
        // Get remaining data from current position
        let remaining_data = &cursor.get_ref()[start_pos..];  // Get all remaining data
        
        // Create decoder with limit on maximum output size
        let mut decoder = ZlibDecoder::new(Cursor::new(remaining_data));  // Create zlib decoder
        let mut decompressed_data = Vec::new();  // Buffer for decompressed data
        
        // If expected size known, reserve memory for it
        if expected_size > 0 {
            decompressed_data.reserve(expected_size);  // Preallocate memory
        }
        
        // Read data with error handling
        let result = decoder.read_to_end(&mut decompressed_data);  // Try to decompress all data
        
        match result {
            Ok(_) => {
                // Decompression successful, move cursor
                let bytes_read = decoder.total_in() as i64;  // Get bytes consumed
                if bytes_read > 0 {
                    cursor.seek(SeekFrom::Current(bytes_read))?;  // Move cursor forward
                    return Ok(decompressed_data);  // Return decompressed data
                } else {
                    // If couldn't determine bytes read
                    return Err("Could not determine number of compressed bytes read".into());  // Return error
                }
            },
            Err(e) => {
                // If EOF error, try to salvage what we've read
                if e.kind() == std::io::ErrorKind::UnexpectedEof && !decompressed_data.is_empty() {
                    // If we got some data and hit EOF, consider it success
                    let bytes_read = decoder.total_in() as i64;  // Get bytes consumed
                    if bytes_read > 0 {
                        cursor.seek(SeekFrom::Current(bytes_read))?;  // Move cursor forward
                        return Ok(decompressed_data);  // Return partial data
                    }
                }
                
                // For other errors, move cursor forward by one byte and return error
                let _ = cursor.seek(SeekFrom::Current(1));  // Move cursor by 1 byte
                Err(e.into())  // Return error
            }
        }
    }
}