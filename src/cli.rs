use std::fmt::Write as _;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use jsonschema::JSONSchema;
use serde::Serialize;
use serde_json::Value;

use crate::decompiler::Decompiler;
use crate::error::Result;
use crate::instruction::{Instruction, Operand};
use crate::manifest::{
    ContractManifest, ManifestPermissionContract, ManifestPermissionMethods, ManifestTrusts,
};
use crate::native_contracts;
use crate::nef::{call_flag_labels, describe_call_flags, MethodToken, NefParser};
use crate::util;

/// Command line interface for the minimal Neo N3 decompiler.
#[derive(Debug, Parser)]
#[command(author, version, about = "Inspect Neo N3 NEF bytecode", long_about = None)]
pub struct Cli {
    /// Optional path to the companion manifest JSON file.
    #[arg(long, global = true)]
    manifest: Option<PathBuf>,

    /// Emit compact JSON (no extra whitespace) whenever `--format json` is requested.
    #[arg(long, global = true)]
    json_compact: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show NEF header information
    Info {
        path: PathBuf,

        /// Choose the output format
        #[arg(long, value_enum, default_value_t = InfoFormat::Text)]
        format: InfoFormat,
    },

    /// Decode bytecode into instructions
    Disasm {
        path: PathBuf,

        /// Choose the output format
        #[arg(long, value_enum, default_value_t = DisasmFormat::Text)]
        format: DisasmFormat,
    },

    /// Parse and pretty-print the bytecode
    Decompile {
        path: PathBuf,

        /// Choose the output view
        #[arg(long, value_enum, default_value_t = DecompileFormat::HighLevel)]
        format: DecompileFormat,
    },

    /// List method tokens embedded in the NEF file
    Tokens {
        path: PathBuf,

        /// Choose the output view
        #[arg(long, value_enum, default_value_t = TokensFormat::Text)]
        format: TokensFormat,
    },

    /// Print one of the bundled JSON schema documents
    Schema(SchemaArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum DecompileFormat {
    Pseudocode,
    #[default]
    HighLevel,
    Both,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum InfoFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum DisasmFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum TokensFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Args)]
struct SchemaArgs {
    /// List available schemas
    #[arg(long, conflicts_with_all = ["schema", "output", "list_json", "validate"])]
    list: bool,

    /// List schemas as a JSON array
    #[arg(long, conflicts_with_all = ["schema", "output", "list", "validate"])]
    list_json: bool,

    /// Schema to print
    #[arg(value_enum)]
    schema: Option<SchemaKind>,

    /// Write the schema to a file instead of stdout
    #[arg(long, requires = "schema")]
    output: Option<PathBuf>,

    /// Skip printing the schema body (shorthand: --quiet)
    #[arg(long, alias = "quiet")]
    no_print: bool,

    /// Validate a JSON file against the specified schema
    #[arg(long, requires = "schema", conflicts_with_all = ["list", "list_json"])]
    validate: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum SchemaKind {
    Info,
    Disasm,
    Decompile,
    Tokens,
}

impl SchemaKind {
    const ALL: [SchemaMetadata; 4] = [
        SchemaMetadata::new(
            SchemaKind::Info,
            SCHEMA_VERSION,
            INFO_SCHEMA,
            "NEF metadata, manifest summary, method tokens, warnings",
        ),
        SchemaMetadata::new(
            SchemaKind::Disasm,
            SCHEMA_VERSION,
            DISASM_SCHEMA,
            "Instruction stream with operand metadata",
        ),
        SchemaMetadata::new(
            SchemaKind::Decompile,
            SCHEMA_VERSION,
            DECOMPILE_SCHEMA,
            "High-level output + pseudocode + disassembly",
        ),
        SchemaMetadata::new(
            SchemaKind::Tokens,
            SCHEMA_VERSION,
            TOKENS_SCHEMA,
            "Standalone method-token listing",
        ),
    ];

    fn as_str(self) -> &'static str {
        match self {
            SchemaKind::Info => "info",
            SchemaKind::Disasm => "disasm",
            SchemaKind::Decompile => "decompile",
            SchemaKind::Tokens => "tokens",
        }
    }

    fn metadata(self) -> SchemaMetadata {
        *Self::ALL
            .iter()
            .find(|entry| entry.kind == self)
            .expect("schema metadata available")
    }
}

#[derive(Clone, Copy)]
struct SchemaMetadata {
    kind: SchemaKind,
    version: &'static str,
    contents: &'static str,
    description: &'static str,
}

impl SchemaMetadata {
    const fn new(
        kind: SchemaKind,
        version: &'static str,
        contents: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            kind,
            version,
            contents,
            description,
        }
    }

    fn report(&self) -> SchemaReport<'_> {
        SchemaReport {
            name: self.kind.as_str(),
            version: self.version,
            description: self.description,
        }
    }
}

#[derive(Serialize)]
struct SchemaReport<'a> {
    name: &'a str,
    version: &'a str,
    description: &'a str,
}

const SCHEMA_VERSION: &str = "1.0.0";

const INFO_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/info.schema.json"
));
const DISASM_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/disasm.schema.json"
));
const DECOMPILE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/decompile.schema.json"
));
const TOKENS_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/tokens.schema.json"
));

impl Cli {
    pub fn run(&self) -> Result<()> {
        match &self.command {
            Command::Info { path, format } => self.run_info(path, *format),
            Command::Disasm { path, format } => self.run_disasm(path, *format),
            Command::Decompile { path, format } => self.run_decompile(path, *format),
            Command::Tokens { path, format } => self.run_tokens(path, *format),
            Command::Schema(args) => self.run_schema(args),
        }
    }

    fn run_info(&self, path: &PathBuf, format: InfoFormat) -> Result<()> {
        let data = std::fs::read(path)?;
        let nef = NefParser::new().parse(&data)?;
        let manifest_path = self.resolve_manifest_path(path);
        let manifest = match manifest_path.as_ref() {
            Some(p) => Some(ContractManifest::from_file(p)?),
            None => None,
        };

        match format {
            InfoFormat::Text => {
                self.print_info_text(path, &nef, manifest.as_ref(), manifest_path.as_ref())
            }
            InfoFormat::Json => {
                self.print_info_json(path, &nef, manifest.as_ref(), manifest_path.as_ref())
            }
        }
    }

    fn print_info_text(
        &self,
        path: &Path,
        nef: &crate::nef::NefFile,
        manifest: Option<&ContractManifest>,
        manifest_path: Option<&PathBuf>,
    ) -> Result<()> {
        println!("File: {}", path.display());
        println!("Compiler: {}", nef.header.compiler);
        if !nef.header.source.is_empty() {
            println!("Source: {}", nef.header.source);
        }
        println!("Script length: {} bytes", nef.script.len());
        let script_hash = nef.script_hash();
        println!("Script hash (LE): {}", util::format_hash(&script_hash));
        println!("Script hash (BE): {}", util::format_hash_be(&script_hash));
        println!("Method tokens: {}", nef.method_tokens.len());
        if !nef.method_tokens.is_empty() {
            println!("Method token entries:");
            for (index, token) in nef.method_tokens.iter().enumerate() {
                println!("    {}", Self::format_method_token_line(index, token));
            }
        }
        println!("Checksum: 0x{:08X}", nef.checksum);

        if let Some(manifest) = manifest {
            println!("Manifest contract: {}", manifest.name);
            if !manifest.supported_standards.is_empty() {
                println!(
                    "Supported standards: {}",
                    manifest.supported_standards.join(", ")
                );
            }
            println!(
                "ABI methods: {} events: {}",
                manifest.abi.methods.len(),
                manifest.abi.events.len()
            );
            println!(
                "Features: storage={} payable={}",
                manifest.features.storage, manifest.features.payable
            );
            if !manifest.groups.is_empty() {
                println!("Groups:");
                for group in &manifest.groups {
                    println!(
                        "    - pubkey={} signature={}",
                        group.pubkey, group.signature
                    );
                }
            }
            if !manifest.permissions.is_empty() {
                println!("Permissions:");
                for permission in &manifest.permissions {
                    println!(
                        "    - contract={} methods={}",
                        permission.contract.describe(),
                        permission.methods.describe()
                    );
                }
            }
            if let Some(trusts) = manifest.trusts.as_ref() {
                println!("Trusts: {}", trusts.describe());
            }
            if let Some(path) = manifest_path {
                println!("Manifest path: {}", path.display());
            }
        }
        Ok(())
    }

    fn run_disasm(&self, path: &PathBuf, format: DisasmFormat) -> Result<()> {
        let decompiler = Decompiler::new();
        let result = decompiler.decompile_file(path)?;
        match format {
            DisasmFormat::Text => {
                for instruction in result.instructions {
                    match instruction.operand {
                        Some(ref operand) => {
                            println!(
                                "{:04X}: {:<10} {}",
                                instruction.offset, instruction.opcode, operand
                            );
                        }
                        None => {
                            println!("{:04X}: {}", instruction.offset, instruction.opcode);
                        }
                    }
                }
            }
            DisasmFormat::Json => {
                let instructions: Vec<InstructionReport> = result
                    .instructions
                    .iter()
                    .map(InstructionReport::from)
                    .collect();
                let report = DisasmReport {
                    file: path.display().to_string(),
                    instructions,
                    warnings: Vec::new(),
                };
                self.print_json(&report)?;
            }
        }
        Ok(())
    }

    fn print_info_json(
        &self,
        path: &Path,
        nef: &crate::nef::NefFile,
        manifest: Option<&ContractManifest>,
        manifest_path: Option<&PathBuf>,
    ) -> Result<()> {
        let script_hash = nef.script_hash();
        let method_tokens: Vec<MethodTokenReport> = nef
            .method_tokens
            .iter()
            .map(Self::build_method_token_report)
            .collect();
        let warnings = Self::collect_warnings(&method_tokens);

        let manifest_summary = manifest.map(summarize_manifest);

        let report = InfoReport {
            file: path.display().to_string(),
            manifest_path: manifest_path.map(|p| p.display().to_string()),
            compiler: nef.header.compiler.clone(),
            source: if nef.header.source.is_empty() {
                None
            } else {
                Some(nef.header.source.clone())
            },
            script_length: nef.script.len(),
            script_hash_le: util::format_hash(&script_hash),
            script_hash_be: util::format_hash_be(&script_hash),
            checksum: format!("0x{:08X}", nef.checksum),
            method_tokens,
            manifest: manifest_summary,
            warnings,
        };

        self.print_json(&report)
    }

    fn run_decompile(&self, path: &PathBuf, format: DecompileFormat) -> Result<()> {
        let decompiler = Decompiler::new();
        let manifest_path = self.resolve_manifest_path(path);
        let result = decompiler.decompile_file_with_manifest(path, manifest_path.as_ref())?;

        match format {
            DecompileFormat::Pseudocode => {
                print!("{}", result.pseudocode);
            }
            DecompileFormat::HighLevel => {
                print!("{}", result.high_level);
            }
            DecompileFormat::Both => {
                println!("// High-level view");
                println!("{}", result.high_level);
                println!("// Pseudocode view");
                print!("{}", result.pseudocode);
            }
            DecompileFormat::Json => {
                let script_hash = result.nef.script_hash();
                let method_tokens: Vec<MethodTokenReport> = result
                    .nef
                    .method_tokens
                    .iter()
                    .map(Self::build_method_token_report)
                    .collect();
                let warnings = Self::collect_warnings(&method_tokens);
                let report = DecompileReport {
                    file: path.display().to_string(),
                    manifest_path: manifest_path
                        .or(self.manifest.clone())
                        .map(|p| p.display().to_string()),
                    script_hash_le: util::format_hash(&script_hash),
                    script_hash_be: util::format_hash_be(&script_hash),
                    high_level: result.high_level.clone(),
                    pseudocode: result.pseudocode.clone(),
                    instructions: result
                        .instructions
                        .iter()
                        .map(InstructionReport::from)
                        .collect(),
                    method_tokens,
                    manifest: result.manifest.as_ref().map(summarize_manifest),
                    warnings,
                };
                self.print_json(&report)?;
            }
        }
        Ok(())
    }

    fn run_tokens(&self, path: &PathBuf, format: TokensFormat) -> Result<()> {
        let data = std::fs::read(path)?;
        let nef = NefParser::new().parse(&data)?;

        if nef.method_tokens.is_empty() {
            match format {
                TokensFormat::Text => println!("(no method tokens)"),
                TokensFormat::Json => {
                    let report = TokensReport {
                        file: path.display().to_string(),
                        method_tokens: Vec::new(),
                        warnings: Vec::new(),
                    };
                    self.print_json(&report)?;
                }
            }
            return Ok(());
        }

        match format {
            TokensFormat::Text => {
                for (index, token) in nef.method_tokens.iter().enumerate() {
                    println!("{}", Self::format_method_token_line(index, token));
                }
            }
            TokensFormat::Json => {
                let tokens = nef
                    .method_tokens
                    .iter()
                    .map(Self::build_method_token_report)
                    .collect::<Vec<_>>();
                let report = TokensReport {
                    file: path.display().to_string(),
                    warnings: Self::collect_warnings(&tokens),
                    method_tokens: tokens,
                };
                self.print_json(&report)?;
            }
        }

        Ok(())
    }

    fn run_schema(&self, args: &SchemaArgs) -> Result<()> {
        if args.list || args.list_json {
            if args.list_json {
                let listing: Vec<_> = SchemaKind::ALL.iter().map(SchemaMetadata::report).collect();
                self.print_json(&listing)?;
            } else {
                for entry in SchemaKind::ALL {
                    println!(
                        "{} v{} - {}",
                        entry.kind.as_str(),
                        entry.version,
                        entry.description
                    );
                }
            }
            return Ok(());
        }

        let schema = args
            .schema
            .expect("--schema <name> is required unless --list/--list-json is set");
        let entry = schema.metadata();
        let value: Value = serde_json::from_str(entry.contents)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        if let Some(target) = args.validate.as_ref() {
            self.validate_against_schema(entry.kind.as_str(), &value, target)?;
        }
        let json = self.render_json(&value)?;
        if !args.no_print {
            println!("{json}");
        }
        if let Some(path) = args.output.as_ref() {
            std::fs::write(path, &json)?;
        }
        Ok(())
    }

    fn validate_against_schema(
        &self,
        schema_name: &str,
        schema_value: &Value,
        path: &Path,
    ) -> Result<()> {
        let compiled = JSONSchema::compile(schema_value)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
        let data = if path == Path::new("-") {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            buf
        } else {
            std::fs::read_to_string(path)?
        };
        let instance: Value = serde_json::from_str(&data)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if let Err(errors) = compiled.validate(&instance) {
            let mut buffer = String::from("schema validation failed:\n");
            for error in errors {
                let mut path = error.instance_path.to_string();
                if path.is_empty() {
                    path.push_str("<root>");
                }
                let _ = writeln!(&mut buffer, "- {path}: {error}");
            }
            return Err(io::Error::new(io::ErrorKind::InvalidData, buffer).into());
        }
        println!(
            "Validation succeeded for {} against {} schema",
            if path == Path::new("-") {
                "stdin".into()
            } else {
                path.display().to_string()
            },
            schema_name
        );
        Ok(())
    }

    fn resolve_manifest_path(&self, nef_path: &Path) -> Option<PathBuf> {
        if let Some(path) = &self.manifest {
            return Some(path.clone());
        }

        let mut candidate = nef_path.to_path_buf();
        candidate.set_extension("manifest.json");
        if candidate.exists() {
            return Some(candidate);
        }

        None
    }

    fn render_json<T: Serialize>(&self, value: &T) -> io::Result<String> {
        if self.json_compact {
            serde_json::to_string(value)
        } else {
            serde_json::to_string_pretty(value)
        }
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    fn print_json<T: Serialize>(&self, value: &T) -> Result<()> {
        let json = self.render_json(value)?;
        println!("{json}");
        Ok(())
    }

    fn format_method_token_line(index: usize, token: &MethodToken) -> String {
        let report = Self::build_method_token_report(token);
        let contract_label = report
            .native_contract
            .as_ref()
            .map(|entry| format!(" ({})", entry.label))
            .unwrap_or_default();
        let warning = report
            .warning
            .as_ref()
            .map(|w| format!(" // warning: {w}"))
            .unwrap_or_default();
        format!(
            "#{index}: hash={}{} method={} params={} returns={} flags=0x{:02X} ({}){}",
            util::format_hash(&token.hash),
            contract_label,
            token.method,
            token.parameters_count,
            token.has_return_value,
            token.call_flags,
            describe_call_flags(token.call_flags),
            warning
        )
    }

    fn build_method_token_report(token: &MethodToken) -> MethodTokenReport {
        let hint = native_contracts::describe_method_token(&token.hash, &token.method);
        let warning = hint.as_ref().and_then(|h| {
            if h.has_exact_method() {
                None
            } else {
                Some(format!(
                    "native contract {} does not expose method {}",
                    h.contract, token.method
                ))
            }
        });
        let native_contract = hint.as_ref().map(|h| NativeContractReport {
            contract: h.contract.to_string(),
            method: h.canonical_method.map(ToString::to_string),
            label: h.formatted_label(&token.method),
        });

        MethodTokenReport {
            method: token.method.clone(),
            hash_le: util::format_hash(&token.hash),
            hash_be: util::format_hash_be(&token.hash),
            parameters: token.parameters_count,
            returns: token.has_return_value,
            call_flags: token.call_flags,
            call_flag_labels: call_flag_labels(token.call_flags),
            native_contract,
            warning,
        }
    }

    fn collect_warnings(tokens: &[MethodTokenReport]) -> Vec<String> {
        tokens
            .iter()
            .filter_map(|report| report.warning.as_ref().map(|w| w.to_string()))
            .collect()
    }
}

fn summarize_manifest(manifest: &ContractManifest) -> ManifestSummary {
    ManifestSummary {
        name: manifest.name.clone(),
        supported_standards: manifest.supported_standards.clone(),
        storage: manifest.features.storage,
        payable: manifest.features.payable,
        groups: manifest
            .groups
            .iter()
            .map(|group| GroupSummary {
                pubkey: group.pubkey.clone(),
                signature: group.signature.clone(),
            })
            .collect(),
        methods: manifest.abi.methods.len(),
        events: manifest.abi.events.len(),
        permissions: manifest
            .permissions
            .iter()
            .map(|permission| PermissionSummary {
                contract: PermissionContractSummary::from(&permission.contract),
                methods: PermissionMethodsSummary::from(&permission.methods),
            })
            .collect(),
        trusts: manifest.trusts.as_ref().map(TrustSummary::from),
        abi: AbiSummary {
            methods: manifest
                .abi
                .methods
                .iter()
                .map(|method| MethodSummary {
                    name: method.name.clone(),
                    parameters: method
                        .parameters
                        .iter()
                        .map(|param| ParameterSummary {
                            name: param.name.clone(),
                            ty: param.kind.clone(),
                        })
                        .collect(),
                    return_type: method.return_type.clone(),
                    safe: method.safe,
                    offset: method.offset,
                })
                .collect(),
            events: manifest
                .abi
                .events
                .iter()
                .map(|event| EventSummary {
                    name: event.name.clone(),
                    parameters: event
                        .parameters
                        .iter()
                        .map(|param| ParameterSummary {
                            name: param.name.clone(),
                            ty: param.kind.clone(),
                        })
                        .collect(),
                })
                .collect(),
        },
    }
}

#[derive(Serialize)]
struct InfoReport {
    file: String,
    manifest_path: Option<String>,
    compiler: String,
    source: Option<String>,
    script_length: usize,
    script_hash_le: String,
    script_hash_be: String,
    checksum: String,
    method_tokens: Vec<MethodTokenReport>,
    manifest: Option<ManifestSummary>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct MethodTokenReport {
    method: String,
    hash_le: String,
    hash_be: String,
    parameters: u16,
    returns: bool,
    call_flags: u8,
    call_flag_labels: Vec<&'static str>,
    native_contract: Option<NativeContractReport>,
    warning: Option<String>,
}

#[derive(Serialize)]
struct NativeContractReport {
    contract: String,
    method: Option<String>,
    label: String,
}

#[derive(Serialize)]
struct ManifestSummary {
    name: String,
    supported_standards: Vec<String>,
    storage: bool,
    payable: bool,
    groups: Vec<GroupSummary>,
    methods: usize,
    events: usize,
    permissions: Vec<PermissionSummary>,
    trusts: Option<TrustSummary>,
    abi: AbiSummary,
}

#[derive(Serialize)]
struct GroupSummary {
    pubkey: String,
    signature: String,
}

#[derive(Serialize)]
struct PermissionSummary {
    contract: PermissionContractSummary,
    methods: PermissionMethodsSummary,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
enum PermissionContractSummary {
    Wildcard(String),
    Hash(String),
    Group(String),
    Other(Value),
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
enum PermissionMethodsSummary {
    Wildcard(String),
    Methods(Vec<String>),
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
enum TrustSummary {
    Wildcard(String),
    Contracts(Vec<String>),
    Other(Value),
}

#[derive(Serialize)]
struct TokensReport {
    file: String,
    method_tokens: Vec<MethodTokenReport>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct DisasmReport {
    file: String,
    instructions: Vec<InstructionReport>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct InstructionReport {
    offset: usize,
    opcode: String,
    operand: Option<String>,
    operand_kind: Option<String>,
    operand_value: Option<OperandValueReport>,
}

impl From<&Instruction> for InstructionReport {
    fn from(instruction: &Instruction) -> Self {
        InstructionReport {
            offset: instruction.offset,
            opcode: instruction.opcode.mnemonic().to_string(),
            operand: instruction.operand.as_ref().map(|op| op.to_string()),
            operand_kind: instruction
                .operand
                .as_ref()
                .map(|op| operand_kind_name(op).to_string()),
            operand_value: instruction.operand.as_ref().map(operand_value_report),
        }
    }
}

fn operand_kind_name(operand: &Operand) -> &'static str {
    match operand {
        Operand::I8(_) => "I8",
        Operand::I16(_) => "I16",
        Operand::I32(_) => "I32",
        Operand::I64(_) => "I64",
        Operand::Bytes(_) => "Bytes",
        Operand::Jump(_) => "Jump8",
        Operand::Jump32(_) => "Jump32",
        Operand::Syscall(_) => "Syscall",
        Operand::U8(_) => "U8",
        Operand::U16(_) => "U16",
        Operand::U32(_) => "U32",
        Operand::Bool(_) => "Bool",
        Operand::Null => "Null",
    }
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
enum OperandValueReport {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    Bool(bool),
    Bytes(String),
    Jump(i32),
    Jump32(i32),
    Syscall(u32),
    Null,
}

fn operand_value_report(operand: &Operand) -> OperandValueReport {
    match operand {
        Operand::I8(value) => OperandValueReport::I8(*value),
        Operand::I16(value) => OperandValueReport::I16(*value),
        Operand::I32(value) => OperandValueReport::I32(*value),
        Operand::I64(value) => OperandValueReport::I64(*value),
        Operand::U8(value) => OperandValueReport::U8(*value),
        Operand::U16(value) => OperandValueReport::U16(*value),
        Operand::U32(value) => OperandValueReport::U32(*value),
        Operand::Bool(value) => OperandValueReport::Bool(*value),
        Operand::Jump(value) => OperandValueReport::Jump(*value as i32),
        Operand::Jump32(value) => OperandValueReport::Jump32(*value),
        Operand::Syscall(value) => OperandValueReport::Syscall(*value),
        Operand::Bytes(bytes) => {
            OperandValueReport::Bytes(format!("0x{}", util::upper_hex_string(bytes)))
        }
        Operand::Null => OperandValueReport::Null,
    }
}

#[derive(Serialize)]
struct DecompileReport {
    file: String,
    manifest_path: Option<String>,
    script_hash_le: String,
    script_hash_be: String,
    high_level: String,
    pseudocode: String,
    instructions: Vec<InstructionReport>,
    method_tokens: Vec<MethodTokenReport>,
    manifest: Option<ManifestSummary>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct AbiSummary {
    methods: Vec<MethodSummary>,
    events: Vec<EventSummary>,
}

#[derive(Serialize)]
struct MethodSummary {
    name: String,
    parameters: Vec<ParameterSummary>,
    return_type: String,
    safe: bool,
    offset: Option<u32>,
}

#[derive(Serialize)]
struct EventSummary {
    name: String,
    parameters: Vec<ParameterSummary>,
}

#[derive(Serialize)]
struct ParameterSummary {
    name: String,
    ty: String,
}

impl From<&ManifestPermissionContract> for PermissionContractSummary {
    fn from(contract: &ManifestPermissionContract) -> Self {
        match contract {
            ManifestPermissionContract::Wildcard(value) => {
                PermissionContractSummary::Wildcard(value.clone())
            }
            ManifestPermissionContract::Hash { hash } => {
                PermissionContractSummary::Hash(hash.clone())
            }
            ManifestPermissionContract::Group { group } => {
                PermissionContractSummary::Group(group.clone())
            }
            ManifestPermissionContract::Other(value) => {
                PermissionContractSummary::Other(value.clone())
            }
        }
    }
}

impl From<&ManifestPermissionMethods> for PermissionMethodsSummary {
    fn from(methods: &ManifestPermissionMethods) -> Self {
        match methods {
            ManifestPermissionMethods::Wildcard(value) => {
                PermissionMethodsSummary::Wildcard(value.clone())
            }
            ManifestPermissionMethods::Methods(list) => {
                PermissionMethodsSummary::Methods(list.clone())
            }
        }
    }
}

impl From<&ManifestTrusts> for TrustSummary {
    fn from(trusts: &ManifestTrusts) -> Self {
        match trusts {
            ManifestTrusts::Wildcard(value) => TrustSummary::Wildcard(value.clone()),
            ManifestTrusts::Contracts(values) => TrustSummary::Contracts(values.clone()),
            ManifestTrusts::Other(value) => TrustSummary::Other(value.clone()),
        }
    }
}
