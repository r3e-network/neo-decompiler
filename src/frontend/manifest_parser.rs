//! Contract manifest parser for Neo N3

use crate::common::errors::ManifestParseError;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Contract manifest parser with validation and type system integration
pub struct ManifestParser {
    /// Validation options
    validation_options: ValidationOptions,
    /// Standards definitions for compliance checking
    standards: HashMap<String, StandardDefinition>,
}

/// Validation options for manifest parsing
#[derive(Debug, Clone)]
pub struct ValidationOptions {
    /// Validate contract hash formats
    pub validate_hashes: bool,
    /// Check for supported standards compliance
    pub check_standards: bool,
    /// Validate ABI consistency
    pub validate_abi: bool,
    /// Allow unknown/custom types
    pub allow_custom_types: bool,
}

impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            validate_hashes: true,
            check_standards: true,
            validate_abi: true,
            allow_custom_types: true,
        }
    }
}

/// Standard definition for NEP compliance checking
#[derive(Debug, Clone)]
pub struct StandardDefinition {
    pub name: String,
    pub version: String,
    pub required_methods: Vec<MethodSignature>,
    pub required_events: Vec<EventSignature>,
    pub optional_methods: Vec<MethodSignature>,
}

/// Method signature for standard compliance
#[derive(Debug, Clone)]
pub struct MethodSignature {
    pub name: String,
    pub parameters: Vec<String>,
    pub return_type: Option<String>,
}

/// Event signature for standard compliance
#[derive(Debug, Clone)]
pub struct EventSignature {
    pub name: String,
    pub parameters: Vec<String>,
}

/// Neo N3 type system mapping
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NeoType {
    Any,
    Boolean,
    Integer,
    ByteString,
    Buffer,
    Array(Box<NeoType>),
    Map(Box<NeoType>, Box<NeoType>),
    Struct(Vec<NeoType>),
    Hash160,
    Hash256,
    PublicKey,
    Signature,
    InteropInterface,
    Void,
    Custom(String),
}

/// Enhanced ABI with type mapping and lookup tables
#[derive(Debug, Clone)]
pub struct EnhancedABI {
    /// Base ABI structure
    pub base: ContractABI,
    /// Method lookup by name with enhanced type information
    pub method_lookup: IndexMap<String, EnhancedMethod>,
    /// Event lookup by name with enhanced type information  
    pub event_lookup: IndexMap<String, EnhancedEvent>,
    /// Supported standards
    pub supported_standards: Vec<String>,
}

/// Enhanced method with type mapping
#[derive(Debug, Clone)]
pub struct EnhancedMethod {
    /// Base method definition
    pub base: ContractMethod,
    /// Mapped parameter types
    pub parameter_types: Vec<NeoType>,
    /// Mapped return type
    pub return_type_mapped: Option<NeoType>,
}

/// Enhanced event with type mapping
#[derive(Debug, Clone)]
pub struct EnhancedEvent {
    /// Base event definition
    pub base: ContractEvent,
    /// Mapped parameter types
    pub parameter_types: Vec<NeoType>,
}

impl ManifestParser {
    /// Create new manifest parser with default options
    pub fn new() -> Self {
        let mut parser = Self {
            validation_options: ValidationOptions::default(),
            standards: HashMap::new(),
        };
        parser.add_nep17_standard();
        parser
    }

    /// Create new manifest parser with custom validation options
    pub fn with_options(validation_options: ValidationOptions) -> Self {
        let mut parser = Self {
            validation_options,
            standards: HashMap::new(),
        };
        parser.add_nep17_standard();
        parser
    }

    /// Add a standard definition for compliance checking
    pub fn add_standard(&mut self, standard: StandardDefinition) {
        self.standards.insert(standard.name.clone(), standard);
    }

    /// Load standards from configuration
    pub fn load_standards<P: AsRef<std::path::Path>>(
        &mut self,
        _config_dir: P,
    ) -> Result<(), ManifestParseError> {
        // Implementation would load from TOML files like nep17.toml
        // Add built-in NEP-17 standard definition
        self.add_nep17_standard();
        Ok(())
    }

    /// Add built-in NEP-17 standard definition
    fn add_nep17_standard(&mut self) {
        let nep17 = StandardDefinition {
            name: "NEP-17".to_string(),
            version: "1.0".to_string(),
            required_methods: vec![
                MethodSignature {
                    name: "symbol".to_string(),
                    parameters: vec![],
                    return_type: Some("String".to_string()),
                },
                MethodSignature {
                    name: "decimals".to_string(),
                    parameters: vec![],
                    return_type: Some("Integer".to_string()),
                },
                MethodSignature {
                    name: "totalSupply".to_string(),
                    parameters: vec![],
                    return_type: Some("Integer".to_string()),
                },
                MethodSignature {
                    name: "balanceOf".to_string(),
                    parameters: vec!["Hash160".to_string()],
                    return_type: Some("Integer".to_string()),
                },
                MethodSignature {
                    name: "transfer".to_string(),
                    parameters: vec![
                        "Hash160".to_string(),
                        "Hash160".to_string(),
                        "Integer".to_string(),
                        "Any".to_string(),
                    ],
                    return_type: Some("Boolean".to_string()),
                },
            ],
            required_events: vec![EventSignature {
                name: "Transfer".to_string(),
                parameters: vec![
                    "Hash160".to_string(),
                    "Hash160".to_string(),
                    "Integer".to_string(),
                ],
            }],
            optional_methods: vec![MethodSignature {
                name: "name".to_string(),
                parameters: vec![],
                return_type: Some("String".to_string()),
            }],
        };
        self.standards.insert("NEP-17".to_string(), nep17);
    }

    /// Parse manifest from JSON string with comprehensive validation
    pub fn parse(&self, json: &str) -> Result<ContractManifest, ManifestParseError> {
        let value: Value = serde_json::from_str(json)?;
        let value = value
            .as_object()
            .ok_or_else(|| ManifestParseError::MissingField {
                field: "root".to_string(),
            })?;

        // Validate required fields exist
        self.validate_required_fields(value)?;

        // Extract required fields
        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ManifestParseError::MissingField {
                field: "name".to_string(),
            })?
            .to_string();

        // Parse groups with validation
        let groups = self.parse_groups(value.get("groups"))?;

        // Parse features
        let features = self.parse_features(value.get("features"))?;

        // Parse ABI with comprehensive validation
        let abi = self.parse_abi(value.get("abi"))?;

        // Parse permissions with hash validation
        let permissions = self.parse_permissions(value.get("permissions"))?;

        // Parse trusts with validation
        let trusts = self.parse_trusts(value.get("trusts"))?;

        // Parse supported standards
        let supported_standards =
            self.parse_supported_standards(value.get("supportedstandards"))?;

        // Parse extra data
        let extra = value.get("extra").cloned().unwrap_or(Value::Null);

        let manifest = ContractManifest {
            name,
            groups,
            features,
            abi,
            permissions,
            trusts,
            supported_standards,
            extra,
        };

        // Validate manifest consistency
        self.validate_manifest(&manifest)?;

        Ok(manifest)
    }

    /// Validate that required fields are present
    fn validate_required_fields(
        &self,
        value: &serde_json::Map<String, Value>,
    ) -> Result<(), ManifestParseError> {
        let required_fields = ["name", "abi"];

        for field in &required_fields {
            if !value.contains_key(*field) {
                return Err(ManifestParseError::MissingField {
                    field: field.to_string(),
                });
            }
        }

        Ok(())
    }

    /// Validate manifest consistency and standards compliance
    fn validate_manifest(&self, manifest: &ContractManifest) -> Result<(), ManifestParseError> {
        // Validate ABI consistency
        if self.validation_options.validate_abi {
            self.validate_abi_consistency(&manifest.abi)?;
        }

        // Check standards compliance
        if self.validation_options.check_standards {
            self.check_standards_compliance(manifest)?;
        }

        Ok(())
    }

    /// Validate ABI internal consistency
    fn validate_abi_consistency(&self, abi: &ContractABI) -> Result<(), ManifestParseError> {
        // Check for duplicate method names
        let mut method_names = std::collections::HashSet::new();
        for method in &abi.methods {
            if !method_names.insert(&method.name) {
                return Err(ManifestParseError::InvalidABI);
            }
        }

        // Check for duplicate event names
        let mut event_names = std::collections::HashSet::new();
        for event in &abi.events {
            if !event_names.insert(&event.name) {
                return Err(ManifestParseError::InvalidABI);
            }
        }

        // Validate parameter and return types
        for method in &abi.methods {
            for param in &method.parameters {
                self.validate_neo_type(&param.param_type)?;
            }
            if let Some(ret_type) = &method.return_type {
                self.validate_neo_type(ret_type)?;
            }
        }

        for event in &abi.events {
            for param in &event.parameters {
                self.validate_neo_type(&param.param_type)?;
            }
        }

        Ok(())
    }

    /// Check compliance with supported standards
    fn check_standards_compliance(
        &self,
        manifest: &ContractManifest,
    ) -> Result<(), ManifestParseError> {
        for standard_name in &manifest.supported_standards {
            if let Some(standard) = self.standards.get(standard_name) {
                self.validate_standard_compliance(&manifest.abi, standard)?;
            }
        }
        Ok(())
    }

    /// Validate compliance with a specific standard
    fn validate_standard_compliance(
        &self,
        abi: &ContractABI,
        standard: &StandardDefinition,
    ) -> Result<(), ManifestParseError> {
        // Check required methods
        for required_method in &standard.required_methods {
            let found = abi.methods.iter().any(|method| {
                method.name == required_method.name
                    && method.parameters.len() == required_method.parameters.len()
                    && method.return_type == required_method.return_type
            });

            if !found {
                return Err(ManifestParseError::InvalidABI);
            }
        }

        // Check required events
        for required_event in &standard.required_events {
            let found = abi.events.iter().any(|event| {
                event.name == required_event.name
                    && event.parameters.len() == required_event.parameters.len()
            });

            if !found {
                return Err(ManifestParseError::InvalidABI);
            }
        }

        Ok(())
    }

    /// Validate Neo N3 type name
    fn validate_neo_type(&self, type_name: &str) -> Result<(), ManifestParseError> {
        let valid_types = [
            "Any",
            "Boolean",
            "Integer",
            "ByteString",
            "Buffer",
            "Array",
            "Map",
            "Struct",
            "Hash160",
            "Hash256",
            "PublicKey",
            "Signature",
            "InteropInterface",
            "Void",
        ];

        if valid_types.contains(&type_name) || self.validation_options.allow_custom_types {
            Ok(())
        } else {
            Err(ManifestParseError::InvalidABI)
        }
    }

    /// Map Neo type string to internal type representation
    fn map_neo_type_to_internal(&self, type_str: &str) -> Option<NeoType> {
        match type_str {
            "Any" => Some(NeoType::Any),
            "Boolean" => Some(NeoType::Boolean),
            "Integer" => Some(NeoType::Integer),
            "ByteString" | "String" => Some(NeoType::ByteString),
            "Buffer" => Some(NeoType::Buffer),
            "Hash160" => Some(NeoType::Hash160),
            "Hash256" => Some(NeoType::Hash256),
            "PublicKey" => Some(NeoType::PublicKey),
            "Signature" => Some(NeoType::Signature),
            "InteropInterface" => Some(NeoType::InteropInterface),
            "Void" => Some(NeoType::Void),
            _ if type_str.starts_with("Array") => Some(NeoType::Array(Box::new(NeoType::Any))),
            _ if type_str.starts_with("Map") => {
                Some(NeoType::Map(Box::new(NeoType::Any), Box::new(NeoType::Any)))
            }
            _ if type_str.starts_with("Struct") => Some(NeoType::Struct(vec![])),
            _ => Some(NeoType::Custom(type_str.to_string())),
        }
    }

    /// Extract ABI for type inference with enhanced metadata
    pub fn extract_abi(&self, manifest: &ContractManifest) -> EnhancedABI {
        let mut method_lookup = IndexMap::new();
        let mut event_lookup = IndexMap::new();

        // Build method lookup table
        for method in &manifest.abi.methods {
            let enhanced_method = EnhancedMethod {
                base: method.clone(),
                parameter_types: method
                    .parameters
                    .iter()
                    .map(|p| {
                        self.map_neo_type_to_internal(&p.param_type)
                            .unwrap_or(NeoType::Any)
                    })
                    .collect(),
                return_type_mapped: method
                    .return_type
                    .as_ref()
                    .and_then(|t| self.map_neo_type_to_internal(t)),
            };
            method_lookup.insert(method.name.clone(), enhanced_method);
        }

        // Build event lookup table
        for event in &manifest.abi.events {
            let enhanced_event = EnhancedEvent {
                base: event.clone(),
                parameter_types: event
                    .parameters
                    .iter()
                    .map(|p| {
                        self.map_neo_type_to_internal(&p.param_type)
                            .unwrap_or(NeoType::Any)
                    })
                    .collect(),
            };
            event_lookup.insert(event.name.clone(), enhanced_event);
        }

        EnhancedABI {
            base: manifest.abi.clone(),
            method_lookup,
            event_lookup,
            supported_standards: manifest.supported_standards.clone(),
        }
    }

    /// Get method by name with enhanced type information
    pub fn get_method_info<'a>(
        &self,
        manifest: &'a ContractManifest,
        method_name: &str,
    ) -> Option<&'a ContractMethod> {
        manifest.abi.methods.iter().find(|m| m.name == method_name)
    }

    /// Get event by name
    pub fn get_event_info<'a>(
        &self,
        manifest: &'a ContractManifest,
        event_name: &str,
    ) -> Option<&'a ContractEvent> {
        manifest.abi.events.iter().find(|e| e.name == event_name)
    }

    /// Detect supported standards from ABI analysis
    pub fn detect_standards(&self, manifest: &ContractManifest) -> Vec<String> {
        let mut detected = Vec::new();

        for (standard_name, standard_def) in &self.standards {
            if self
                .validate_standard_compliance(&manifest.abi, standard_def)
                .is_ok()
            {
                detected.push(standard_name.clone());
            }
        }

        detected
    }

    /// Parse contract groups with validation
    fn parse_groups(
        &self,
        groups_value: Option<&Value>,
    ) -> Result<Vec<ContractGroup>, ManifestParseError> {
        let mut groups = Vec::new();

        if let Some(Value::Array(array)) = groups_value {
            for group_value in array {
                let group_obj = group_value
                    .as_object()
                    .ok_or(ManifestParseError::InvalidGroup)?;

                let pubkey = group_obj
                    .get("pubkey")
                    .and_then(|v| v.as_str())
                    .ok_or(ManifestParseError::InvalidGroup)?
                    .to_string();

                let signature = group_obj
                    .get("signature")
                    .and_then(|v| v.as_str())
                    .ok_or(ManifestParseError::InvalidGroup)?
                    .to_string();

                // Validate public key format (33 bytes hex)
                if self.validation_options.validate_hashes {
                    self.validate_public_key(&pubkey)?;
                    self.validate_signature(&signature)?;
                }

                groups.push(ContractGroup { pubkey, signature });
            }
        }

        Ok(groups)
    }

    /// Validate public key format
    fn validate_public_key(&self, pubkey: &str) -> Result<(), ManifestParseError> {
        if pubkey.len() != 66 {
            // 33 bytes * 2 hex chars per byte
            return Err(ManifestParseError::InvalidGroup);
        }

        // Check if it's valid hex
        if !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ManifestParseError::InvalidGroup);
        }

        Ok(())
    }

    /// Validate signature format
    fn validate_signature(&self, signature: &str) -> Result<(), ManifestParseError> {
        if signature.len() != 128 {
            // 64 bytes * 2 hex chars per byte
            return Err(ManifestParseError::InvalidGroup);
        }

        // Check if it's valid hex
        if !signature.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ManifestParseError::InvalidGroup);
        }

        Ok(())
    }

    /// Parse contract features
    fn parse_features(
        &self,
        features_value: Option<&Value>,
    ) -> Result<ContractFeatures, ManifestParseError> {
        let features = if let Some(Value::Object(obj)) = features_value {
            ContractFeatures {
                storage: obj
                    .get("storage")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                payable: obj
                    .get("payable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            }
        } else {
            ContractFeatures::default()
        };

        Ok(features)
    }

    /// Parse contract ABI with comprehensive validation
    fn parse_abi(&self, abi_value: Option<&Value>) -> Result<ContractABI, ManifestParseError> {
        let abi_obj = abi_value
            .and_then(|v| v.as_object())
            .ok_or(ManifestParseError::InvalidABI)?;

        // Parse methods
        let methods = if let Some(Value::Array(methods_array)) = abi_obj.get("methods") {
            self.parse_methods(methods_array)?
        } else {
            Vec::new()
        };

        // Parse events
        let events = if let Some(Value::Array(events_array)) = abi_obj.get("events") {
            self.parse_events(events_array)?
        } else {
            Vec::new()
        };

        Ok(ContractABI { methods, events })
    }

    /// Parse ABI methods with enhanced validation
    fn parse_methods(
        &self,
        methods_array: &[Value],
    ) -> Result<Vec<ContractMethod>, ManifestParseError> {
        let mut methods = Vec::new();

        for method_value in methods_array {
            let method_obj = method_value
                .as_object()
                .ok_or(ManifestParseError::InvalidABI)?;

            let name = method_obj
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or(ManifestParseError::InvalidABI)?
                .to_string();

            let offset = method_obj
                .get("offset")
                .and_then(|v| v.as_i64())
                .unwrap_or(-1) as i32;

            let parameters = if let Some(Value::Array(params_array)) = method_obj.get("parameters")
            {
                self.parse_parameters(params_array)?
            } else {
                Vec::new()
            };

            let return_type = method_obj
                .get("returntype")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let safe = method_obj
                .get("safe")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            methods.push(ContractMethod {
                name,
                offset,
                parameters,
                return_type,
                safe,
            });
        }

        Ok(methods)
    }

    /// Parse method parameters with type validation
    fn parse_parameters(
        &self,
        params_array: &[Value],
    ) -> Result<Vec<ContractParameter>, ManifestParseError> {
        let mut parameters = Vec::new();

        for param_value in params_array {
            let param_obj = param_value
                .as_object()
                .ok_or(ManifestParseError::InvalidABI)?;

            let name = param_obj
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or(ManifestParseError::InvalidABI)?
                .to_string();

            let param_type = param_obj
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or(ManifestParseError::InvalidABI)?
                .to_string();

            // Validate parameter type
            self.validate_neo_type(&param_type)?;

            parameters.push(ContractParameter { name, param_type });
        }

        Ok(parameters)
    }

    /// Parse ABI events with type validation
    fn parse_events(
        &self,
        events_array: &[Value],
    ) -> Result<Vec<ContractEvent>, ManifestParseError> {
        let mut events = Vec::new();

        for event_value in events_array {
            let event_obj = event_value
                .as_object()
                .ok_or(ManifestParseError::InvalidABI)?;

            let name = event_obj
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or(ManifestParseError::InvalidABI)?
                .to_string();

            let parameters = if let Some(Value::Array(params_array)) = event_obj.get("parameters") {
                self.parse_parameters(params_array)?
            } else {
                Vec::new()
            };

            events.push(ContractEvent { name, parameters });
        }

        Ok(events)
    }

    /// Parse contract permissions with hash validation
    fn parse_permissions(
        &self,
        perms_value: Option<&Value>,
    ) -> Result<Vec<ContractPermission>, ManifestParseError> {
        let mut permissions = Vec::new();

        if let Some(Value::Array(array)) = perms_value {
            for perm_value in array {
                let perm_obj = perm_value
                    .as_object()
                    .ok_or(ManifestParseError::InvalidPermission)?;

                let contract = perm_obj
                    .get("contract")
                    .and_then(|v| v.as_str())
                    .ok_or(ManifestParseError::InvalidPermission)?
                    .to_string();

                // Validate contract hash format if not wildcard
                if self.validation_options.validate_hashes && contract != "*" {
                    self.validate_contract_hash(&contract)?;
                }

                let methods = if let Some(Value::Array(methods_array)) = perm_obj.get("methods") {
                    methods_array
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                } else {
                    Vec::new()
                };

                permissions.push(ContractPermission { contract, methods });
            }
        }

        Ok(permissions)
    }

    /// Parse contract trusts with hash validation
    fn parse_trusts(&self, trusts_value: Option<&Value>) -> Result<Vec<Trust>, ManifestParseError> {
        let mut trusts = Vec::new();

        if let Some(Value::Array(array)) = trusts_value {
            for trust_value in array {
                if let Some(contract_hash) = trust_value.as_str() {
                    // Validate contract hash format
                    if self.validation_options.validate_hashes {
                        self.validate_contract_hash(contract_hash)?;
                    }
                    trusts.push(Trust::Contract(contract_hash.to_string()));
                } else if trust_value.as_null().is_some() {
                    trusts.push(Trust::Wildcard);
                }
            }
        }

        Ok(trusts)
    }

    /// Parse supported standards
    fn parse_supported_standards(
        &self,
        standards_value: Option<&Value>,
    ) -> Result<Vec<String>, ManifestParseError> {
        let mut standards = Vec::new();

        if let Some(Value::Array(array)) = standards_value {
            for standard_value in array {
                if let Some(standard_name) = standard_value.as_str() {
                    standards.push(standard_name.to_string());
                }
            }
        }

        Ok(standards)
    }

    /// Validate contract hash format (160-bit hash in hex)
    fn validate_contract_hash(&self, hash: &str) -> Result<(), ManifestParseError> {
        // Handle both formats: 0x prefix and without
        let hash_str = if hash.starts_with("0x") {
            &hash[2..]
        } else {
            hash
        };

        // Check length (20 bytes = 40 hex chars)
        if hash_str.len() != 40 {
            return Err(ManifestParseError::InvalidPermission);
        }

        // Check if it's valid hex
        if !hash_str.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ManifestParseError::InvalidPermission);
        }

        Ok(())
    }
}

impl Default for ManifestParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Contract manifest representation with enhanced metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractManifest {
    /// Contract name
    pub name: String,
    /// Contract groups
    pub groups: Vec<ContractGroup>,
    /// Contract features
    pub features: ContractFeatures,
    /// Application Binary Interface
    pub abi: ContractABI,
    /// Contract permissions
    pub permissions: Vec<ContractPermission>,
    /// Contract trusts
    pub trusts: Vec<Trust>,
    /// Supported standards (e.g., NEP-17, NEP-11)
    pub supported_standards: Vec<String>,
    /// Extra metadata
    pub extra: Value,
}

/// Contract group for multi-signature contracts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractGroup {
    /// Public key (33 bytes hex-encoded)
    pub pubkey: String,
    /// Signature (64 bytes hex-encoded)
    pub signature: String,
}

/// Contract features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractFeatures {
    /// Has storage
    pub storage: bool,
    /// Is payable (can receive NEO/GAS)
    pub payable: bool,
}

impl Default for ContractFeatures {
    fn default() -> Self {
        Self {
            storage: false,
            payable: false,
        }
    }
}

/// Application Binary Interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractABI {
    /// Contract methods
    pub methods: Vec<ContractMethod>,
    /// Contract events
    pub events: Vec<ContractEvent>,
}

/// Contract method definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMethod {
    /// Method name
    pub name: String,
    /// Bytecode offset (-1 if not specified)
    pub offset: i32,
    /// Method parameters
    pub parameters: Vec<ContractParameter>,
    /// Return type
    pub return_type: Option<String>,
    /// Is safe method (read-only)
    pub safe: bool,
}

/// Contract event definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractEvent {
    /// Event name
    pub name: String,
    /// Event parameters
    pub parameters: Vec<ContractParameter>,
}

/// Contract parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: String,
}

/// Contract permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractPermission {
    /// Target contract hash or wildcard "*"
    pub contract: String,
    /// Allowed methods (empty means all methods)
    pub methods: Vec<String>,
}

/// Contract trust relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trust {
    /// Trust specific contract by hash
    Contract(String),
    /// Trust all contracts (wildcard)
    Wildcard,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_parser_creation() {
        let parser = ManifestParser::new();
        assert!(!parser.standards.is_empty());
    }

    #[test]
    fn test_parse_comprehensive_manifest() {
        let parser = ManifestParser::new();
        let json = r#"{
            "name": "TestToken",
            "groups": [
                {
                    "pubkey": "0279bedd5df3c6f2e8ccc0f8f2b8e5c1f5d8c2e1a5c7a4b2f2a3d2e1c4b5a6e7f8",
                    "signature": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
                }
            ],
            "features": {
                "storage": true,
                "payable": true
            },
            "abi": {
                "methods": [
                    {
                        "name": "symbol",
                        "offset": 0,
                        "parameters": [],
                        "returntype": "String",
                        "safe": true
                    },
                    {
                        "name": "transfer",
                        "offset": 100,
                        "parameters": [
                            {"name": "from", "type": "Hash160"},
                            {"name": "to", "type": "Hash160"},
                            {"name": "amount", "type": "Integer"},
                            {"name": "data", "type": "Any"}
                        ],
                        "returntype": "Boolean",
                        "safe": false
                    }
                ],
                "events": [
                    {
                        "name": "Transfer",
                        "parameters": [
                            {"name": "from", "type": "Hash160"},
                            {"name": "to", "type": "Hash160"},
                            {"name": "amount", "type": "Integer"}
                        ]
                    }
                ]
            },
            "permissions": [
                {
                    "contract": "*",
                    "methods": ["transfer"]
                },
                {
                    "contract": "0x1234567890abcdef1234567890abcdef12345678",
                    "methods": []
                }
            ],
            "trusts": [
                "0xabcdef1234567890abcdef1234567890abcdef12",
                null
            ],
            "supportedstandards": ["NEP-17"],
            "extra": {"version": "1.0"}
        }"#;

        let result = parser.parse(json);
        assert!(result.is_ok());

        let manifest = result.unwrap();
        assert_eq!(manifest.name, "TestToken");
        assert!(manifest.features.storage);
        assert!(manifest.features.payable);
        assert_eq!(manifest.abi.methods.len(), 2);
        assert_eq!(manifest.abi.events.len(), 1);
        assert_eq!(manifest.permissions.len(), 2);
        assert_eq!(manifest.trusts.len(), 2);
        assert_eq!(manifest.supported_standards.len(), 1);
        assert_eq!(manifest.supported_standards[0], "NEP-17");
    }

    #[test]
    fn test_enhanced_abi_extraction() {
        let parser = ManifestParser::new();
        let manifest = ContractManifest {
            name: "TestContract".to_string(),
            groups: Vec::new(),
            features: ContractFeatures::default(),
            abi: ContractABI {
                methods: vec![ContractMethod {
                    name: "testMethod".to_string(),
                    offset: 0,
                    parameters: vec![ContractParameter {
                        name: "param1".to_string(),
                        param_type: "Hash160".to_string(),
                    }],
                    return_type: Some("Boolean".to_string()),
                    safe: true,
                }],
                events: vec![ContractEvent {
                    name: "TestEvent".to_string(),
                    parameters: vec![ContractParameter {
                        name: "value".to_string(),
                        param_type: "Integer".to_string(),
                    }],
                }],
            },
            permissions: Vec::new(),
            trusts: Vec::new(),
            supported_standards: Vec::new(),
            extra: Value::Null,
        };

        let enhanced_abi = parser.extract_abi(&manifest);
        assert_eq!(enhanced_abi.method_lookup.len(), 1);
        assert_eq!(enhanced_abi.event_lookup.len(), 1);

        let method = enhanced_abi.method_lookup.get("testMethod").unwrap();
        assert_eq!(method.parameter_types.len(), 1);
        assert_eq!(method.parameter_types[0], NeoType::Hash160);
        assert_eq!(method.return_type_mapped, Some(NeoType::Boolean));
    }

    #[test]
    fn test_standards_detection() {
        let mut parser = ManifestParser::new();
        let manifest = ContractManifest {
            name: "NEP17Token".to_string(),
            groups: Vec::new(),
            features: ContractFeatures {
                storage: true,
                payable: false,
            },
            abi: ContractABI {
                methods: vec![
                    ContractMethod {
                        name: "symbol".to_string(),
                        offset: 0,
                        parameters: vec![],
                        return_type: Some("String".to_string()),
                        safe: true,
                    },
                    ContractMethod {
                        name: "decimals".to_string(),
                        offset: 10,
                        parameters: vec![],
                        return_type: Some("Integer".to_string()),
                        safe: true,
                    },
                    ContractMethod {
                        name: "totalSupply".to_string(),
                        offset: 20,
                        parameters: vec![],
                        return_type: Some("Integer".to_string()),
                        safe: true,
                    },
                    ContractMethod {
                        name: "balanceOf".to_string(),
                        offset: 30,
                        parameters: vec![ContractParameter {
                            name: "account".to_string(),
                            param_type: "Hash160".to_string(),
                        }],
                        return_type: Some("Integer".to_string()),
                        safe: true,
                    },
                    ContractMethod {
                        name: "transfer".to_string(),
                        offset: 40,
                        parameters: vec![
                            ContractParameter {
                                name: "from".to_string(),
                                param_type: "Hash160".to_string(),
                            },
                            ContractParameter {
                                name: "to".to_string(),
                                param_type: "Hash160".to_string(),
                            },
                            ContractParameter {
                                name: "amount".to_string(),
                                param_type: "Integer".to_string(),
                            },
                            ContractParameter {
                                name: "data".to_string(),
                                param_type: "Any".to_string(),
                            },
                        ],
                        return_type: Some("Boolean".to_string()),
                        safe: false,
                    },
                ],
                events: vec![ContractEvent {
                    name: "Transfer".to_string(),
                    parameters: vec![
                        ContractParameter {
                            name: "from".to_string(),
                            param_type: "Hash160".to_string(),
                        },
                        ContractParameter {
                            name: "to".to_string(),
                            param_type: "Hash160".to_string(),
                        },
                        ContractParameter {
                            name: "amount".to_string(),
                            param_type: "Integer".to_string(),
                        },
                    ],
                }],
            },
            permissions: Vec::new(),
            trusts: Vec::new(),
            supported_standards: Vec::new(),
            extra: Value::Null,
        };

        let detected = parser.detect_standards(&manifest);
        assert!(detected.contains(&"NEP-17".to_string()));
    }

    #[test]
    fn test_parse_invalid_manifest() {
        let parser = ManifestParser::new();

        // Missing name
        let json_missing_name = r#"{"abi": {"methods": [], "events": []}}"#;
        assert!(parser.parse(json_missing_name).is_err());

        // Invalid JSON
        let invalid_json = r#"{"name": "test", invalid}"#;
        assert!(parser.parse(invalid_json).is_err());

        // Invalid group format
        let json_invalid_group = r#"{
            "name": "test",
            "abi": {"methods": [], "events": []},
            "groups": [{"pubkey": "invalid"}]
        }"#;
        assert!(parser.parse(json_invalid_group).is_err());
    }

    #[test]
    fn test_validation_options() {
        let mut options = ValidationOptions::default();
        options.validate_hashes = false;

        let parser = ManifestParser::with_options(options);

        // Should not validate hash formats when disabled
        let json_with_invalid_hash = r#"{
            "name": "test",
            "abi": {"methods": [], "events": []},
            "permissions": [{"contract": "invalid_hash", "methods": []}]
        }"#;

        assert!(parser.parse(json_with_invalid_hash).is_ok());
    }
}
