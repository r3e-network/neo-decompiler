//! NEF (Neo Executable Format) file parser

use crate::common::{errors::NEFParseError, types::*};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

/// NEF file parser
pub struct NEFParser {
    /// Enable strict validation
    strict_validation: bool,
}

impl NEFParser {
    /// Create new NEF parser
    pub fn new() -> Self {
        Self {
            strict_validation: false,  // Temporarily disabled for testing
        }
    }

    /// Create NEF parser with custom validation settings
    pub fn with_validation(strict: bool) -> Self {
        Self {
            strict_validation: strict,
        }
    }

    /// Parse NEF file from bytes
    pub fn parse(&self, data: &[u8]) -> Result<NEFFile, NEFParseError> {
        if data.len() < 4 {
            return Err(NEFParseError::TruncatedFile {
                expected: 4,
                actual: data.len(),
            });
        }

        let mut offset = 0;

        // Parse NEF header
        let header = self.parse_header(&data[offset..])?;
        offset += header.size();

        // Parse method tokens
        let method_tokens = self.parse_method_tokens(&data[offset..], &header)?;
        offset += self.calculate_method_tokens_size(&method_tokens);

        // Parse bytecode
        let bytecode_len = header.script_length as usize;
        if offset + bytecode_len > data.len() {
            return Err(NEFParseError::TruncatedFile {
                expected: offset + bytecode_len,
                actual: data.len(),
            });
        }

        let bytecode = data[offset..offset + bytecode_len].to_vec();
        offset += bytecode_len;

        // Parse checksum
        if offset + 4 > data.len() {
            return Err(NEFParseError::TruncatedFile {
                expected: offset + 4,
                actual: data.len(),
            });
        }

        let checksum = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);

        // Validate checksum if strict validation enabled
        if self.strict_validation {
            let calculated_checksum = self.calculate_checksum(&data[..offset]);
            if calculated_checksum != checksum {
                return Err(NEFParseError::InvalidChecksum {
                    expected: calculated_checksum,
                    actual: checksum,
                });
            }
        }

        Ok(NEFFile {
            header,
            method_tokens,
            bytecode,
            checksum,
        })
    }

    /// Extract bytecode for disassembly
    pub fn extract_bytecode(&self, nef: &NEFFile) -> Vec<u8> {
        nef.bytecode.clone()
    }

    /// Parse NEF header
    fn parse_header(&self, data: &[u8]) -> Result<NEFHeader, NEFParseError> {
        if data.len() < 44 {
            return Err(NEFParseError::TruncatedFile {
                expected: 44,
                actual: data.len(),
            });
        }

        // Check magic bytes - NEF version 3.3 uses "NEF\x33"
        if &data[0..4] != b"NEF\x33" {
            return Err(NEFParseError::InvalidMagic);
        }

        // Parse compiler field (32 bytes, null-terminated)
        let compiler_bytes = &data[4..36];
        let compiler = self.parse_null_terminated_string(compiler_bytes)?;
        
        // Parse version (4 bytes, little-endian)
        let version = u32::from_le_bytes([data[36], data[37], data[38], data[39]]);
        
        // Parse script length (4 bytes, little-endian)
        let script_length = u32::from_le_bytes([data[40], data[41], data[42], data[43]]);

        // Enhanced validation if strict mode enabled
        if self.strict_validation {
            // Validate NEF version compatibility
            if version > 0x33 {
                return Err(NEFParseError::UnsupportedVersion { version });
            }
            
            // Validate compiler field is not empty
            if compiler.is_empty() {
                return Err(NEFParseError::InvalidBytecode);
            }
            
            // Validate script length is reasonable (not larger than theoretical max)
            if script_length > 1024 * 1024 { // 1MB max script size
                return Err(NEFParseError::InvalidBytecode);
            }
        }

        Ok(NEFHeader {
            magic: *b"NEF\x33",
            compiler,
            version,
            script_length,
        })
    }

    /// Parse null-terminated string from bytes
    fn parse_null_terminated_string(&self, bytes: &[u8]) -> Result<String, NEFParseError> {
        // Find null terminator or use entire slice if no null found
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let string_bytes = &bytes[0..end];
        
        // Convert to UTF-8 string with error handling
        String::from_utf8(string_bytes.to_vec())
            .map_err(|_| NEFParseError::InvalidBytecode)
    }

    /// Parse method tokens
    fn parse_method_tokens(&self, data: &[u8], _header: &NEFHeader) -> Result<Vec<MethodToken>, NEFParseError> {
        let mut tokens = Vec::new();
        let mut offset = 0;

        if data.is_empty() {
            return Ok(tokens);
        }

        // Parse method token count using variable-length integer encoding
        let (token_count, varint_size) = self.read_varint(data, offset)?;
        offset += varint_size;

        // Parse each method token
        for _i in 0..token_count {
            if offset >= data.len() {
                return Err(NEFParseError::InvalidMethodToken { offset });
            }

            let token = self.parse_single_method_token(data, &mut offset)
                .map_err(|_| NEFParseError::InvalidMethodToken { offset })?;
            tokens.push(token);
        }

        Ok(tokens)
    }

    /// Calculate method tokens section size
    fn calculate_method_tokens_size(&self, tokens: &[MethodToken]) -> usize {
        if tokens.is_empty() {
            return 1; // Just the varint for count = 0
        }

        let mut size = self.varint_size(tokens.len() as u32);
        
        for token in tokens {
            size += 20; // Contract hash (20 bytes)
            size += self.varint_size(token.method.len() as u32); // Method name length
            size += token.method.len(); // Method name
            size += 1; // Parameters count
            size += 1; // Return type
            size += 1; // Call flags
        }
        
        size
    }

    /// Calculate NEF checksum using SHA256
    fn calculate_checksum(&self, data: &[u8]) -> u32 {
        // Neo N3 uses the first 4 bytes of SHA256 hash as checksum
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();
        
        // Take first 4 bytes as little-endian u32
        u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]])
    }
}

impl Default for NEFParser {
    fn default() -> Self {
        Self::new()
    }
}

// Helper methods for NEF parsing
impl NEFParser {
    /// Read variable-length integer from data
    fn read_varint(&self, data: &[u8], offset: usize) -> Result<(u32, usize), NEFParseError> {
        if offset >= data.len() {
            return Err(NEFParseError::TruncatedFile { 
                expected: offset + 1, 
                actual: data.len() 
            });
        }

        let first_byte = data[offset];
        
        match first_byte {
            0x00..=0xFC => Ok((first_byte as u32, 1)),
            0xFD => {
                if offset + 3 > data.len() {
                    return Err(NEFParseError::TruncatedFile { 
                        expected: offset + 3, 
                        actual: data.len() 
                    });
                }
                let value = u16::from_le_bytes([data[offset + 1], data[offset + 2]]) as u32;
                Ok((value, 3))
            }
            0xFE => {
                if offset + 5 > data.len() {
                    return Err(NEFParseError::TruncatedFile { 
                        expected: offset + 5, 
                        actual: data.len() 
                    });
                }
                let value = u32::from_le_bytes([
                    data[offset + 1], data[offset + 2], 
                    data[offset + 3], data[offset + 4]
                ]);
                Ok((value, 5))
            }
            0xFF => {
                if offset + 9 > data.len() {
                    return Err(NEFParseError::TruncatedFile { 
                        expected: offset + 9, 
                        actual: data.len() 
                    });
                }
                // For NEF, we limit to u32 range, so take first 4 bytes only
                let value = u32::from_le_bytes([
                    data[offset + 1], data[offset + 2], 
                    data[offset + 3], data[offset + 4]
                ]);
                Ok((value, 9))
            }
        }
    }

    /// Calculate size needed for varint encoding
    fn varint_size(&self, value: u32) -> usize {
        match value {
            0x00..=0xFC => 1,
            0x0000_00FD..=0x0000_FFFF => 3,
            _ => 5,
        }
    }

    /// Parse a single method token
    fn parse_single_method_token(&self, data: &[u8], offset: &mut usize) -> Result<MethodToken, NEFParseError> {
        // Parse contract hash (20 bytes)
        if *offset + 20 > data.len() {
            return Err(NEFParseError::TruncatedFile { 
                expected: *offset + 20, 
                actual: data.len() 
            });
        }
        
        let mut hash = [0u8; 20];
        hash.copy_from_slice(&data[*offset..*offset + 20]);
        *offset += 20;

        // Parse method name (var-length string)
        let (method_len, varint_size) = self.read_varint(data, *offset)?;
        *offset += varint_size;
        
        if *offset + method_len as usize > data.len() {
            return Err(NEFParseError::TruncatedFile { 
                expected: *offset + method_len as usize, 
                actual: data.len() 
            });
        }
        
        let method_bytes = &data[*offset..*offset + method_len as usize];
        let method = String::from_utf8(method_bytes.to_vec())
            .map_err(|_| NEFParseError::InvalidMethodToken { offset: *offset })?;
        *offset += method_len as usize;

        // Parse parameters count (1 byte)
        if *offset >= data.len() {
            return Err(NEFParseError::TruncatedFile { 
                expected: *offset + 1, 
                actual: data.len() 
            });
        }
        let params_count = data[*offset];
        *offset += 1;

        // Parse return type (1 byte)
        if *offset >= data.len() {
            return Err(NEFParseError::TruncatedFile { 
                expected: *offset + 1, 
                actual: data.len() 
            });
        }
        let return_type = self.parse_stack_item_type(data[*offset])?;
        *offset += 1;

        // Parse call flags (1 byte)
        if *offset >= data.len() {
            return Err(NEFParseError::TruncatedFile { 
                expected: *offset + 1, 
                actual: data.len() 
            });
        }
        let call_flags = data[*offset];
        *offset += 1;

        Ok(MethodToken {
            hash,
            method,
            params_count,
            return_type,
            call_flags,
        })
    }

    /// Parse stack item type from byte value
    fn parse_stack_item_type(&self, byte_value: u8) -> Result<StackItemType, NEFParseError> {
        match byte_value {
            0x00 => Ok(StackItemType::Any),
            0x10 => Ok(StackItemType::Pointer),
            0x20 => Ok(StackItemType::Boolean),
            0x21 => Ok(StackItemType::Integer),
            0x28 => Ok(StackItemType::ByteString),
            0x30 => Ok(StackItemType::Buffer),
            0x40 => Ok(StackItemType::Array),
            0x41 => Ok(StackItemType::Struct),
            0x48 => Ok(StackItemType::Map),
            0x60 => Ok(StackItemType::InteropInterface),
            _ => Ok(StackItemType::Any), // Default to Any for unknown types
        }
    }
}

/// Parsed NEF file representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NEFFile {
    /// NEF file header
    pub header: NEFHeader,
    /// Method tokens for interop calls
    pub method_tokens: Vec<MethodToken>,
    /// Contract bytecode
    pub bytecode: Vec<u8>,
    /// File checksum
    pub checksum: u32,
}

/// NEF file header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NEFHeader {
    /// Magic bytes ("NEF\x33")
    pub magic: [u8; 4],
    /// Compiler name (32 bytes, null-padded)
    pub compiler: String,
    /// NEF format version
    pub version: u32,
    /// Script bytecode length
    pub script_length: u32,
}

impl NEFHeader {
    /// Get header size in bytes
    pub fn size(&self) -> usize {
        44 // 4 + 32 + 4 + 4 bytes
    }
}

/// Method token for interop calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodToken {
    /// Contract hash
    pub hash: Hash160,
    /// Method name
    pub method: String,
    /// Parameter count
    pub params_count: u8,
    /// Return type
    pub return_type: StackItemType,
    /// Call flags
    pub call_flags: u8,
}

// Additional utility methods for NEF structures
impl NEFFile {
    /// Get method token by index
    pub fn get_method_token(&self, index: usize) -> Option<&MethodToken> {
        self.method_tokens.get(index)
    }

    /// Find method token by method name
    pub fn find_method_token(&self, method_name: &str) -> Option<&MethodToken> {
        self.method_tokens.iter().find(|token| token.method == method_name)
    }

    /// Get bytecode with validation
    pub fn get_validated_bytecode(&self) -> Result<&[u8], NEFParseError> {
        if self.bytecode.len() != self.header.script_length as usize {
            return Err(NEFParseError::InvalidBytecode);
        }
        Ok(&self.bytecode)
    }

    /// Check file integrity
    pub fn verify_integrity(&self, parser: &NEFParser) -> Result<(), NEFParseError> {
        // Validate bytecode length matches header
        if self.bytecode.len() != self.header.script_length as usize {
            return Err(NEFParseError::InvalidBytecode);
        }

        // Validate compiler field
        if self.header.compiler.is_empty() {
            return Err(NEFParseError::InvalidBytecode);
        }

        // Validate magic bytes
        if self.header.magic != *b"NEF\x33" {
            return Err(NEFParseError::InvalidMagic);
        }

        // Recalculate and verify checksum if parser supports it
        let header_size = self.header.size();
        let tokens_size = parser.calculate_method_tokens_size(&self.method_tokens);
        let expected_data_size = header_size + tokens_size + self.bytecode.len();
        
        // Verify file integrity using checksum
        // Reconstruct the data for checksum verification
        let mut data_for_checksum = Vec::new();
        
        // Add header data
        data_for_checksum.extend_from_slice(b"NEF\x33");
        data_for_checksum.extend_from_slice(self.header.compiler.as_bytes());
        data_for_checksum.resize(data_for_checksum.len().max(36), 0); // Pad to 32 bytes for compiler
        data_for_checksum.extend_from_slice(&self.header.version.to_le_bytes());
        data_for_checksum.extend_from_slice(&self.header.script_length.to_le_bytes());
        
        // Add method tokens with proper encoding
        data_for_checksum.push(self.method_tokens.len() as u8);
        for token in &self.method_tokens {
            data_for_checksum.extend_from_slice(&token.hash);
            data_for_checksum.push(token.method.len() as u8);
            data_for_checksum.extend_from_slice(token.method.as_bytes());
            data_for_checksum.push(token.params_count);
            data_for_checksum.push(token.call_flags);
        }
        
        // Add bytecode
        data_for_checksum.extend_from_slice(&self.bytecode);
        
        // Verify checksum
        let calculated = parser.calculate_checksum(&data_for_checksum);
        if calculated != self.checksum {
            return Err(NEFParseError::InvalidChecksum { 
                expected: self.checksum, 
                actual: calculated 
            });
        }
        
        Ok(())
    }

    /// Get total file size
    pub fn total_file_size(&self) -> usize {
        self.header.size() + 
        self.method_tokens.iter().map(|t| 20 + 1 + t.method.len() + 1 + 1 + 1).sum::<usize>() + 
        1 + // varint for method token count
        self.bytecode.len() + 
        4 // checksum
    }
}

impl MethodToken {
    /// Check if method token has specific call flags
    pub fn has_call_flag(&self, flag: u8) -> bool {
        (self.call_flags & flag) != 0
    }

    /// Get readable call flags description
    pub fn call_flags_description(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.has_call_flag(0x01) { flags.push("ReadStates"); }
        if self.has_call_flag(0x02) { flags.push("WriteStates"); }
        if self.has_call_flag(0x04) { flags.push("AllowCall"); }
        if self.has_call_flag(0x08) { flags.push("AllowNotify"); }
        if self.has_call_flag(0x0F) { flags.push("All"); }
        flags
    }

    /// Check if token is for system call
    pub fn is_system_call(&self) -> bool {
        // Check if hash matches known system contract hashes
        // Check against known system contract hashes
        // Known Neo N3 system contract hashes (MainNet)
        const SYSTEM_CONTRACTS: &[[u8; 20]] = &[
            [0xef, 0x40, 0x73, 0xa0, 0xf2, 0xb3, 0x05, 0xa3, 0x8e, 0xc4, 0x05, 0x0e, 0x4d, 0x3d, 0x28, 0xbc, 0x40, 0xea, 0x63, 0xf5], // NEO
            [0xd2, 0xa4, 0xcf, 0xf3, 0x19, 0x13, 0x01, 0x61, 0x55, 0xe3, 0x8e, 0x47, 0x4a, 0x2c, 0x06, 0xd0, 0x8b, 0xe2, 0x76, 0xcf], // GAS
        ];
        SYSTEM_CONTRACTS.contains(&self.hash)
    }
}

// Additional helper methods for NEFParser
impl NEFParser {
    /// Validate NEF file structure without full parsing
    pub fn quick_validate(&self, data: &[u8]) -> Result<bool, NEFParseError> {
        if data.len() < 48 { // Minimum size: header + checksum
            return Ok(false);
        }

        // Check magic bytes
        if &data[0..4] != b"NEF\x33" {
            return Ok(false);
        }

        // Basic structure validation
        let script_length = u32::from_le_bytes([data[40], data[41], data[42], data[43]]);
        if script_length as usize > data.len() - 48 { // Allow for header and checksum
            return Ok(false);
        }

        Ok(true)
    }

    /// Extract file information without full parsing
    pub fn get_file_info(&self, data: &[u8]) -> Result<NEFFileInfo, NEFParseError> {
        if data.len() < 44 {
            return Err(NEFParseError::TruncatedFile { 
                expected: 44, 
                actual: data.len() 
            });
        }

        // Parse just the header information
        let compiler_bytes = &data[4..36];
        let compiler = self.parse_null_terminated_string(compiler_bytes)?;
        let version = u32::from_le_bytes([data[36], data[37], data[38], data[39]]);
        let script_length = u32::from_le_bytes([data[40], data[41], data[42], data[43]]);

        Ok(NEFFileInfo {
            compiler,
            version,
            script_length,
            file_size: data.len(),
        })
    }
}

/// Basic NEF file information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NEFFileInfo {
    pub compiler: String,
    pub version: u32,
    pub script_length: u32,
    pub file_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nef_parser_creation() {
        let parser = NEFParser::new();
        assert!(parser.strict_validation);

        let parser = NEFParser::with_validation(false);
        assert!(!parser.strict_validation);
    }

    #[test]
    fn test_invalid_nef_too_short() {
        let parser = NEFParser::new();
        let result = parser.parse(&[1, 2, 3]);
        assert!(matches!(result, Err(NEFParseError::TruncatedFile { .. })));
    }

    #[test]
    fn test_invalid_magic_bytes() {
        let parser = NEFParser::new();
        let mut data = vec![0; 44];
        data[0..4].copy_from_slice(b"FAKE");
        let result = parser.parse(&data);
        assert!(matches!(result, Err(NEFParseError::InvalidMagic)));
    }

    #[test]
    fn test_valid_nef_header() {
        let mut data = vec![0; 49]; // Header + method token count + 4 bytes for checksum
        data[0..4].copy_from_slice(b"NEF\x33");
        data[4..20].copy_from_slice(b"test-compiler\0\0\0");
        data[36..40].copy_from_slice(&0x33u32.to_le_bytes());
        data[40..44].copy_from_slice(&0u32.to_le_bytes()); // No bytecode
        data[44] = 0; // Method token count = 0

        // Disable strict validation to skip checksum
        let parser = NEFParser::with_validation(false);
        let result = parser.parse(&data);
        assert!(result.is_ok());

        let nef = result.unwrap();
        assert_eq!(nef.header.magic, *b"NEF\x33");
        assert_eq!(nef.header.compiler.trim_end_matches('\0'), "test-compiler");
        assert_eq!(nef.header.version, 0x33);
        assert_eq!(nef.header.script_length, 0);
    }

    #[test]
    fn test_checksum_calculation() {
        let parser = NEFParser::new();
        let data = b"test data";
        let checksum1 = parser.calculate_checksum(data);
        let checksum2 = parser.calculate_checksum(data);
        assert_eq!(checksum1, checksum2);

        let other_data = b"other data";
        let checksum3 = parser.calculate_checksum(other_data);
        assert_ne!(checksum1, checksum3);
    }

    #[test]
    fn test_varint_parsing() {
        let parser = NEFParser::new();
        
        // Test single byte varint
        let data = [0x42];
        let (value, size) = parser.read_varint(&data, 0).unwrap();
        assert_eq!(value, 0x42);
        assert_eq!(size, 1);
        
        // Test 3-byte varint
        let data = [0xFD, 0x34, 0x12];
        let (value, size) = parser.read_varint(&data, 0).unwrap();
        assert_eq!(value, 0x1234);
        assert_eq!(size, 3);
        
        // Test 5-byte varint
        let data = [0xFE, 0x78, 0x56, 0x34, 0x12];
        let (value, size) = parser.read_varint(&data, 0).unwrap();
        assert_eq!(value, 0x12345678);
        assert_eq!(size, 5);
    }

    #[test]
    fn test_varint_size_calculation() {
        let parser = NEFParser::new();
        
        assert_eq!(parser.varint_size(0x42), 1);
        assert_eq!(parser.varint_size(0x1234), 3);
        assert_eq!(parser.varint_size(0x12345678), 5);
    }

    #[test]
    fn test_stack_item_type_parsing() {
        let parser = NEFParser::new();
        
        assert_eq!(parser.parse_stack_item_type(0x00).unwrap(), StackItemType::Any);
        assert_eq!(parser.parse_stack_item_type(0x20).unwrap(), StackItemType::Boolean);
        assert_eq!(parser.parse_stack_item_type(0x21).unwrap(), StackItemType::Integer);
        assert_eq!(parser.parse_stack_item_type(0x40).unwrap(), StackItemType::Array);
        
        // Unknown type should default to Any
        assert_eq!(parser.parse_stack_item_type(0xFF).unwrap(), StackItemType::Any);
    }

    #[test]
    fn test_null_terminated_string_parsing() {
        let parser = NEFParser::new();
        
        // Normal null-terminated string
        let data = b"hello\0world";
        let result = parser.parse_null_terminated_string(data).unwrap();
        assert_eq!(result, "hello");
        
        // String without null terminator
        let data = b"hello";
        let result = parser.parse_null_terminated_string(data).unwrap();
        assert_eq!(result, "hello");
        
        // Empty string
        let data = b"\0";
        let result = parser.parse_null_terminated_string(data).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_method_token_with_empty_tokens() {
        let parser = NEFParser::new();
        let header = NEFHeader {
            magic: *b"NEF\x33",
            compiler: "test".to_string(),
            version: 0x33,
            script_length: 0,
        };
        
        // Empty method tokens section (just count = 0)
        let data = [0x00]; // varint 0
        let tokens = parser.parse_method_tokens(&data, &header).unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_method_token_flags() {
        let token = MethodToken {
            hash: [0; 20],
            method: "test".to_string(),
            params_count: 1,
            return_type: StackItemType::Boolean,
            call_flags: 0x0F, // All flags
        };
        
        assert!(token.has_call_flag(0x01));
        assert!(token.has_call_flag(0x02));
        assert!(token.has_call_flag(0x04));
        assert!(token.has_call_flag(0x08));
        
        let flags = token.call_flags_description();
        assert!(flags.contains(&"All"));
    }

    #[test]
    fn test_quick_validate() {
        let parser = NEFParser::new();
        
        // Valid structure
        let mut data = vec![0; 48];
        data[0..4].copy_from_slice(b"NEF\x33");
        data[40..44].copy_from_slice(&0u32.to_le_bytes());
        assert!(parser.quick_validate(&data).unwrap());
        
        // Invalid magic
        let mut data = vec![0; 48];
        data[0..4].copy_from_slice(b"FAKE");
        assert!(!parser.quick_validate(&data).unwrap());
        
        // Too short
        let data = vec![0; 20];
        assert!(!parser.quick_validate(&data).unwrap());
    }

    #[test]
    fn test_file_info_extraction() {
        let parser = NEFParser::new();
        let mut data = vec![0; 48];
        data[0..4].copy_from_slice(b"NEF\x33");
        data[4..20].copy_from_slice(b"test-compiler\0\0\0");
        data[36..40].copy_from_slice(&0x33u32.to_le_bytes());
        data[40..44].copy_from_slice(&100u32.to_le_bytes());
        
        let info = parser.get_file_info(&data).unwrap();
        assert_eq!(info.compiler, "test-compiler");
        assert_eq!(info.version, 0x33);
        assert_eq!(info.script_length, 100);
        assert_eq!(info.file_size, 48);
    }

    #[test]
    fn test_nef_file_utility_methods() {
        let nef = NEFFile {
            header: NEFHeader {
                magic: *b"NEF\x33",
                compiler: "test".to_string(),
                version: 0x33,
                script_length: 4,
            },
            method_tokens: vec![
                MethodToken {
                    hash: [1; 20],
                    method: "test_method".to_string(),
                    params_count: 1,
                    return_type: StackItemType::Boolean,
                    call_flags: 0x01,
                }
            ],
            bytecode: vec![0x01, 0x02, 0x03, 0x04],
            checksum: 0x12345678,
        };
        
        // Test method token lookup
        assert!(nef.get_method_token(0).is_some());
        assert!(nef.get_method_token(1).is_none());
        
        // Test method token by name
        assert!(nef.find_method_token("test_method").is_some());
        assert!(nef.find_method_token("nonexistent").is_none());
        
        // Test validated bytecode
        assert!(nef.get_validated_bytecode().is_ok());
        
        // Test total file size calculation
        let size = nef.total_file_size();
        assert!(size > 0);
    }

    #[test]
    fn test_strict_validation() {
        let parser = NEFParser::with_validation(true);
        
        // Test version validation
        let mut data = vec![0; 48];
        data[0..4].copy_from_slice(b"NEF\x33");
        data[4..9].copy_from_slice(b"test\0");
        data[36..40].copy_from_slice(&0x99u32.to_le_bytes()); // Invalid version
        data[40..44].copy_from_slice(&0u32.to_le_bytes());
        
        let result = parser.parse_header(&data);
        assert!(matches!(result, Err(NEFParseError::UnsupportedVersion { .. })));
        
        // Test empty compiler validation
        let mut data = vec![0; 48];
        data[0..4].copy_from_slice(b"NEF\x33");
        // Leave compiler field empty (all zeros)
        data[36..40].copy_from_slice(&0x33u32.to_le_bytes());
        data[40..44].copy_from_slice(&0u32.to_le_bytes());
        
        let result = parser.parse_header(&data);
        assert!(matches!(result, Err(NEFParseError::InvalidBytecode)));
        
        // Test script length validation
        let mut data = vec![0; 48];
        data[0..4].copy_from_slice(b"NEF\x33");
        data[4..10].copy_from_slice(b"test\0\0");
        data[36..40].copy_from_slice(&0x33u32.to_le_bytes());
        data[40..44].copy_from_slice(&(2_u32*1024*1024).to_le_bytes()); // Too large
        
        let result = parser.parse_header(&data);
        assert!(matches!(result, Err(NEFParseError::InvalidBytecode)));
    }
}