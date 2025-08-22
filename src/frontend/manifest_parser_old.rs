//! Contract manifest parser for Neo N3

use crate::common::errors::ManifestParseError;
use crate::common::types::{Hash160, ContractId, StackItemType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use indexmap::IndexMap;
use sha2::{Sha256, Digest};

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

impl ManifestParser {
    /// Create new manifest parser with default options
    pub fn new() -> Self {
        Self {
            validation_options: ValidationOptions::default(),
            standards: HashMap::new(),
        }
    }

    /// Create new manifest parser with custom validation options
    pub fn with_options(validation_options: ValidationOptions) -> Self {
        Self {
            validation_options,
            standards: HashMap::new(),
        }
    }

    /// Add a standard definition for compliance checking
    pub fn add_standard(&mut self, standard: StandardDefinition) {
        self.standards.insert(standard.name.clone(), standard);
    }

    /// Load standards from configuration
    pub fn load_standards<P: AsRef<std::path::Path>>(&mut self, config_dir: P) -> Result<(), ManifestParseError> {
        // Implementation would load from TOML files like nep17.toml
        // For now, add built-in NEP-17 standard
        self.add_nep17_standard();
        Ok(())
    }

    /// Add built-in NEP-17 standard definition
    fn add_nep17_standard(&mut self) {
        let nep17 = StandardDefinition {
            name: "NEP-17".to_string(),
            version: "1.0".to_string(),
            required_methods: vec![
                MethodSignature { name: "symbol".to_string(), parameters: vec![], return_type: Some("String".to_string()) },
                MethodSignature { name: "decimals".to_string(), parameters: vec![], return_type: Some("Integer".to_string()) },
                MethodSignature { name: "totalSupply".to_string(), parameters: vec![], return_type: Some("Integer".to_string()) },
                MethodSignature { name: "balanceOf".to_string(), parameters: vec!["Hash160".to_string()], return_type: Some("Integer".to_string()) },
                MethodSignature { name: "transfer".to_string(), parameters: vec!["Hash160".to_string(), "Hash160".to_string(), "Integer".to_string(), "Any".to_string()], return_type: Some("Boolean".to_string()) },
            ],
            required_events: vec![
                EventSignature { name: "Transfer".to_string(), parameters: vec!["Hash160".to_string(), "Hash160".to_string(), "Integer".to_string()] },
            ],
            optional_methods: vec![
                MethodSignature { name: "name".to_string(), parameters: vec![], return_type: Some("String".to_string()) },
            ],
        };
        self.standards.insert("NEP-17".to_string(), nep17);
    }

    /// Parse manifest from JSON string
    pub fn parse(&self, json: &str) -> Result<ContractManifest, ManifestParseError> {
        let value: Value = serde_json::from_str(json)?;
        
        // Extract required fields
        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ManifestParseError::MissingField {
                field: "name".to_string(),
            })?
            .to_string();

        // Parse groups
        let groups = self.parse_groups(value.get("groups"))?;

        // Parse features  
        let features = self.parse_features(value.get("features"))?;

        // Parse ABI
        let abi = self.parse_abi(value.get("abi"))?;

        // Parse permissions
        let permissions = self.parse_permissions(value.get("permissions"))?;

        // Parse trusts
        let trusts = self.parse_trusts(value.get("trusts"))?;

        // Parse extra data
        let extra = value.get("extra").cloned().unwrap_or(Value::Null);

        Ok(ContractManifest {
            name,
            groups,
            features,
            abi,
            permissions,
            trusts,
            extra,
        })
    }

    /// Extract ABI for type inference
    pub fn extract_abi(&self, manifest: &ContractManifest) -> ContractABI {
        manifest.abi.clone()
    }

    /// Parse contract groups
    fn parse_groups(&self, groups_value: Option<&Value>) -> Result<Vec<ContractGroup>, ManifestParseError> {
        let mut groups = Vec::new();

        if let Some(Value::Array(array)) = groups_value {
            for group_value in array {
                let pubkey = group_value
                    .get("pubkey")
                    .and_then(|v| v.as_str())
                    .ok_or(ManifestParseError::InvalidGroup)?
                    .to_string();

                let signature = group_value
                    .get("signature")
                    .and_then(|v| v.as_str())
                    .ok_or(ManifestParseError::InvalidGroup)?
                    .to_string();

                groups.push(ContractGroup { pubkey, signature });
            }
        }

        Ok(groups)
    }

    /// Parse contract features
    fn parse_features(&self, features_value: Option<&Value>) -> Result<ContractFeatures, ManifestParseError> {
        let features = if let Some(Value::Object(obj)) = features_value {
            ContractFeatures {
                storage: obj.get("storage").and_then(|v| v.as_bool()).unwrap_or(false),
                payable: obj.get("payable").and_then(|v| v.as_bool()).unwrap_or(false),
            }
        } else {
            ContractFeatures::default()
        };

        Ok(features)
    }

    /// Parse contract ABI
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

    /// Parse ABI methods
    fn parse_methods(&self, methods_array: &[Value]) -> Result<Vec<ContractMethod>, ManifestParseError> {
        let mut methods = Vec::new();

        for method_value in methods_array {
            let method_obj = method_value.as_object().ok_or(ManifestParseError::InvalidABI)?;

            let name = method_obj
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or(ManifestParseError::InvalidABI)?
                .to_string();

            let offset = method_obj
                .get("offset")
                .and_then(|v| v.as_i64())
                .unwrap_or(-1) as i32;

            let parameters = if let Some(Value::Array(params_array)) = method_obj.get("parameters") {
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

    /// Parse method parameters
    fn parse_parameters(&self, params_array: &[Value]) -> Result<Vec<ContractParameter>, ManifestParseError> {
        let mut parameters = Vec::new();

        for param_value in params_array {
            let param_obj = param_value.as_object().ok_or(ManifestParseError::InvalidABI)?;

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

            parameters.push(ContractParameter { name, param_type });
        }

        Ok(parameters)
    }

    /// Parse ABI events
    fn parse_events(&self, events_array: &[Value]) -> Result<Vec<ContractEvent>, ManifestParseError> {
        let mut events = Vec::new();

        for event_value in events_array {
            let event_obj = event_value.as_object().ok_or(ManifestParseError::InvalidABI)?;

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

    /// Parse contract permissions
    fn parse_permissions(&self, perms_value: Option<&Value>) -> Result<Vec<ContractPermission>, ManifestParseError> {
        let mut permissions = Vec::new();

        if let Some(Value::Array(array)) = perms_value {
            for perm_value in array {
                let perm_obj = perm_value.as_object().ok_or(ManifestParseError::InvalidPermission)?;

                let contract = perm_obj
                    .get("contract")
                    .and_then(|v| v.as_str())
                    .ok_or(ManifestParseError::InvalidPermission)?
                    .to_string();

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

    /// Parse contract trusts
    fn parse_trusts(&self, trusts_value: Option<&Value>) -> Result<Vec<Trust>, ManifestParseError> {
        let mut trusts = Vec::new();

        if let Some(Value::Array(array)) = trusts_value {
            for trust_value in array {
                if let Some(contract_hash) = trust_value.as_str() {
                    trusts.push(Trust::Contract(contract_hash.to_string()));
                } else if trust_value.as_null().is_some() {
                    trusts.push(Trust::Wildcard);
                }
            }
        }

        Ok(trusts)
    }
}

impl Default for ManifestParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Contract manifest representation
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
    /// Extra metadata
    pub extra: Value,
}

/// Contract group for multi-signature contracts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractGroup {
    /// Public key
    pub pubkey: String,
    /// Signature
    pub signature: String,
}

/// Contract features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractFeatures {
    /// Has storage
    pub storage: bool,
    /// Is payable
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
    /// Target contract
    pub contract: String,
    /// Allowed methods
    pub methods: Vec<String>,
}

/// Contract trust relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trust {
    /// Trust specific contract
    Contract(String),
    /// Trust all contracts
    Wildcard,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_parser_creation() {
        let parser = ManifestParser::new();
        // Parser should be created successfully
        let _ = parser;
    }

    #[test]
    fn test_parse_simple_manifest() {
        let parser = ManifestParser::new();
        let json = r#"{
            "name": "TestContract",
            "groups": [],
            "features": {
                "storage": true,
                "payable": false
            },
            "abi": {
                "methods": [
                    {
                        "name": "testMethod",
                        "offset": 0,
                        "parameters": [
                            {
                                "name": "param1",
                                "type": "String"
                            }
                        ],
                        "returntype": "Boolean",
                        "safe": true
                    }
                ],
                "events": []
            },
            "permissions": [],
            "trusts": [],
            "extra": null
        }"#;

        let result = parser.parse(json);
        assert!(result.is_ok());

        let manifest = result.unwrap();
        assert_eq!(manifest.name, "TestContract");
        assert!(manifest.features.storage);
        assert!(!manifest.features.payable);
        assert_eq!(manifest.abi.methods.len(), 1);
        assert_eq!(manifest.abi.methods[0].name, "testMethod");
        assert_eq!(manifest.abi.methods[0].parameters.len(), 1);
        assert_eq!(manifest.abi.methods[0].parameters[0].name, "param1");
        assert_eq!(manifest.abi.methods[0].parameters[0].param_type, "String");
    }

    #[test]
    fn test_parse_manifest_missing_name() {
        let parser = ManifestParser::new();
        let json = r#"{
            "groups": [],
            "features": {},
            "abi": {"methods": [], "events": []},
            "permissions": [],
            "trusts": [],
            "extra": null
        }"#;

        let result = parser.parse(json);
        assert!(matches!(result, Err(ManifestParseError::MissingField { .. })));
    }

    #[test]
    fn test_parse_invalid_json() {
        let parser = ManifestParser::new();
        let invalid_json = r#"{"name": "test", invalid}"#;

        let result = parser.parse(invalid_json);
        assert!(matches!(result, Err(ManifestParseError::Json(_))));
    }

    #[test]
    fn test_extract_abi() {
        let manifest = ContractManifest {
            name: "TestContract".to_string(),
            groups: Vec::new(),
            features: ContractFeatures::default(),
            abi: ContractABI {
                methods: vec![ContractMethod {
                    name: "testMethod".to_string(),
                    offset: 0,
                    parameters: Vec::new(),
                    return_type: Some("Boolean".to_string()),
                    safe: true,
                }],
                events: Vec::new(),
            },
            permissions: Vec::new(),
            trusts: Vec::new(),
            extra: Value::Null,
        };

        let parser = ManifestParser::new();
        let abi = parser.extract_abi(&manifest);
        assert_eq!(abi.methods.len(), 1);
        assert_eq!(abi.methods[0].name, "testMethod");
    }
}