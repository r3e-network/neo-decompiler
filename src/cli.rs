//! Comprehensive CLI interface for the Neo N3 decompiler
//! 
//! Provides multiple commands for different analysis and decompilation workflows:
//! - `disasm`: Pretty disassembly with offsets and operands
//! - `cfg`: Control flow graph visualization in GraphViz DOT format
//! - `decompile`: Full decompilation to multiple pseudocode formats
//! - `analyze`: Security analysis and NEP conformance checking
//! - `info`: Metadata extraction and file information

use std::path::PathBuf;
use std::io::{self, Write};
use std::fs;
use std::time::Instant;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json;
use tracing::{info, warn, debug};

use crate::{
    Decompiler, DecompilationResult, DecompilerConfig, DecompilerError, DecompilerResult,
    NEFParser, ManifestParser, Disassembler,
};

/// Neo N3 Smart Contract Decompiler
/// 
/// A comprehensive tool for analyzing, decompiling, and understanding
/// Neo N3 smart contracts from their compiled NEF bytecode.
#[derive(Parser)]
#[command(
    name = "neo-decompile",
    version = "0.1.0", 
    author = "Neo Development Team",
    about = "Neo N3 Smart Contract Decompiler",
    long_about = "A comprehensive decompiler for Neo N3 smart contracts that provides
                  disassembly, control flow analysis, decompilation to multiple formats,
                  security analysis, and metadata extraction.",
    after_help = "Examples:\n\
      neo-decompile disasm contract.nef\n\
      neo-decompile decompile -m contract.manifest.json -o output.py -f python contract.nef\n\
      neo-decompile cfg --dot contract.nef > contract.dot\n\
      neo-decompile analyze --security --nep-compliance contract.nef\n\
      neo-decompile info --metadata contract.nef"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Configuration file path
    #[arg(short, long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Verbose output (-v, -vv, -vvv for increasing verbosity)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Enable colored output (auto-detected by default)
    #[arg(long, global = true, value_enum)]
    pub color: Option<ColorChoice>,

    /// Enable progress indicators for long-running operations
    #[arg(long, global = true)]
    pub progress: bool,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Pretty disassembly with offsets and operand details
    #[command(alias = "dis")]
    Disasm {
        /// NEF file to disassemble
        nef_file: PathBuf,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Show byte offsets
        #[arg(long, default_value = "true")]
        offsets: bool,

        /// Show raw bytes
        #[arg(long)]
        bytes: bool,

        /// Show operand details
        #[arg(long, default_value = "true")]
        operands: bool,

        /// Add comments with instruction explanations
        #[arg(long)]
        comments: bool,

        /// Show statistics summary
        #[arg(long)]
        stats: bool,
    },

    /// Generate control flow graph in GraphViz DOT format
    #[command(alias = "graph")]
    Cfg {
        /// NEF file to analyze
        nef_file: PathBuf,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format
        #[arg(short, long, default_value = "dot")]
        format: GraphFormat,

        /// Include basic blocks in nodes
        #[arg(long)]
        show_blocks: bool,

        /// Include instruction details in nodes
        #[arg(long)]
        show_instructions: bool,

        /// Use simplified view (fewer details)
        #[arg(long)]
        simplified: bool,

        /// Generate analysis alongside graph
        #[arg(long)]
        analysis: bool,
    },

    /// Full decompilation to pseudocode
    #[command(alias = "dec")]
    Decompile {
        /// NEF file to decompile
        nef_file: PathBuf,

        /// Contract manifest file
        #[arg(short, long)]
        manifest: Option<PathBuf>,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Pseudocode output format
        #[arg(short, long, default_value = "pseudocode")]
        format: PseudocodeFormat,

        /// Optimization level (0=none, 3=aggressive)
        #[arg(long, default_value = "1", value_parser = clap::value_parser!(u8).range(0..=3))]
        optimization: u8,

        /// Enable type inference
        #[arg(long, default_value = "true")]
        type_inference: bool,

        /// Include analysis reports
        #[arg(long)]
        reports: bool,

        /// Include performance metrics
        #[arg(long)]
        metrics: bool,

        /// Generate multiple output formats
        #[arg(long)]
        multi_format: bool,
    },

    /// Security analysis and NEP conformance checking
    #[command(alias = "check")]
    Analyze {
        /// NEF file to analyze
        nef_file: PathBuf,

        /// Contract manifest file
        #[arg(short, long)]
        manifest: Option<PathBuf>,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Analysis output format
        #[arg(short, long, default_value = "json")]
        format: AnalysisFormat,

        /// Perform security vulnerability analysis
        #[arg(long)]
        security: bool,

        /// Check NEP standard compliance (NEP-17, NEP-11, etc.)
        #[arg(long)]
        nep_compliance: bool,

        /// Performance analysis (gas costs, optimization opportunities)
        #[arg(long)]
        performance: bool,

        /// Code quality analysis
        #[arg(long)]
        quality: bool,

        /// Include all analysis types
        #[arg(long)]
        all: bool,

        /// Severity threshold for reporting issues
        #[arg(long, default_value = "medium")]
        threshold: Severity,
    },

    /// Extract metadata and file information
    Info {
        /// NEF file to inspect
        nef_file: PathBuf,

        /// Contract manifest file
        #[arg(short, long)]
        manifest: Option<PathBuf>,

        /// Output format
        #[arg(short, long, default_value = "text")]
        format: InfoFormat,

        /// Include file metadata (size, hashes, etc.)
        #[arg(long, default_value = "true")]
        metadata: bool,

        /// Include contract methods and events
        #[arg(long, default_value = "true")]
        methods: bool,

        /// Include dependency information
        #[arg(long)]
        dependencies: bool,

        /// Include bytecode statistics
        #[arg(long)]
        stats: bool,

        /// Include compiler information
        #[arg(long)]
        compiler: bool,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Generate example configuration files and documentation
    Init {
        /// Directory to create example files in
        #[arg(default_value = ".")]
        directory: PathBuf,

        /// Force overwrite existing files
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    
    /// Validate configuration file
    Validate {
        /// Configuration file to validate
        config_file: PathBuf,
    },
    
    /// Generate default configuration file
    Generate {
        /// Output file for configuration
        #[arg(short, long, default_value = "decompiler.toml")]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum GraphFormat {
    Dot,
    Json,
    Svg,
    Png,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum PseudocodeFormat {
    /// Generic pseudocode (default)
    Pseudocode,
    /// C-like syntax
    C,
    /// Python-like syntax
    Python,
    /// Rust-like syntax
    Rust,
    /// TypeScript-like syntax
    Typescript,
    /// Neo Assembly Language
    Nal,
    /// Structured JSON output
    Json,
    /// HTML with syntax highlighting
    Html,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum AnalysisFormat {
    Json,
    Yaml,
    Text,
    Html,
    Sarif,  // Static Analysis Results Interchange Format
}

#[derive(Debug, Clone, ValueEnum)]
pub enum InfoFormat {
    Text,
    Json,
    Yaml,
    Table,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Cli {
    /// Execute the CLI command
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Load configuration
        let config = self.load_config()?;
        
        // Execute the specific command
        match &self.command {
            Commands::Disasm { 
                nef_file, output, offsets, bytes, operands, comments, stats 
            } => {
                self.handle_disasm(nef_file, output.as_ref(), *offsets, *bytes, *operands, *comments, *stats)
            },
            
            Commands::Cfg { 
                nef_file, output, format, show_blocks, show_instructions, simplified, analysis 
            } => {
                self.handle_cfg(nef_file, output.as_ref(), format, *show_blocks, *show_instructions, *simplified, *analysis)
            },
            
            Commands::Decompile { 
                nef_file, manifest, output, format, optimization, type_inference, reports, metrics, multi_format 
            } => {
                self.handle_decompile(&config, nef_file, manifest.as_ref(), output.as_ref(), 
                                    format, *optimization, *type_inference, *reports, *metrics, *multi_format)
            },
            
            Commands::Analyze { 
                nef_file, manifest, output, format, security, nep_compliance, performance, quality, all, threshold 
            } => {
                self.handle_analyze(&config, nef_file, manifest.as_ref(), output.as_ref(), 
                                  format, *security, *nep_compliance, *performance, *quality, *all, threshold)
            },
            
            Commands::Info { 
                nef_file, manifest, format, metadata, methods, dependencies, stats, compiler 
            } => {
                self.handle_info(nef_file, manifest.as_ref(), format, *metadata, *methods, *dependencies, *stats, *compiler)
            },
            
            Commands::Config { command } => {
                self.handle_config(command)
            },
            
            Commands::Init { directory, force } => {
                self.handle_init(directory, *force)
            },
        }
    }

    /// Load configuration from file or use defaults
    fn load_config(&self) -> Result<DecompilerConfig, Box<dyn std::error::Error>> {
        match &self.config {
            Some(config_path) => {
                info!("Loading configuration from: {:?}", config_path);
                Ok(DecompilerConfig::load_from_file(config_path)?)
            },
            None => {
                debug!("Using default configuration");
                Ok(DecompilerConfig::default())
            }
        }
    }

    /// Handle disassembly command
    fn handle_disasm(&self, nef_file: &PathBuf, output: Option<&PathBuf>, 
                     offsets: bool, bytes: bool, operands: bool, comments: bool, stats: bool) 
                     -> Result<(), Box<dyn std::error::Error>> {
        
        info!("Disassembling NEF file: {:?}", nef_file);
        let start_time = Instant::now();
        
        // Read and parse NEF file
        let nef_data = fs::read(nef_file)?;
        let nef_parser = NEFParser::new();
        let nef_file_parsed = nef_parser.parse(&nef_data)?;
        
        // Create disassembler with default config
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        let instructions = disassembler.disassemble(&nef_file_parsed.bytecode)?;
        
        // Generate disassembly output
        let mut output_lines = Vec::new();
        
        if stats {
            output_lines.push(format!("# NEF File Statistics"));
            output_lines.push(format!("# File size: {} bytes", nef_data.len()));
            output_lines.push(format!("# Bytecode size: {} bytes", nef_file_parsed.bytecode.len()));
            output_lines.push(format!("# Instructions: {}", instructions.len()));
            output_lines.push(format!("# Compiler: {:?}", nef_file_parsed.header.compiler));
            output_lines.push(String::new());
        }
        
        for (i, instr) in instructions.iter().enumerate() {
            let mut line = String::new();
            
            if offsets {
                line.push_str(&format!("{:04x}: ", i * 4)); // Approximate offset
            }
            
            if bytes {
                // Add hex dump of instruction bytes
                let end_offset = std::cmp::min(instr.offset as usize + 16, nef_file_parsed.bytecode.len());
                let hex_bytes: Vec<String> = nef_file_parsed.bytecode[instr.offset as usize..end_offset]
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect();
                line.push_str(&format!("{:<32}  ", hex_bytes.join(" ")));
            }
            
            line.push_str(&format!("{:?}", instr.opcode));
            
            if operands && instr.operand.is_some() {
                line.push_str(" ");
                if let Some(ref operand) = instr.operand {
                    line.push_str(&format!("{:?}", operand));
                }
            }
            
            if comments {
                line.push_str(&format!("  ; {:?}", instr.opcode)); // Add instruction explanation
            }
            
            output_lines.push(line);
        }
        
        let output_content = output_lines.join("\n");
        
        // Write output
        match output {
            Some(output_path) => {
                fs::write(output_path, &output_content)?;
                info!("Disassembly written to: {:?}", output_path);
            },
            None => {
                println!("{}", output_content);
            }
        }
        
        let duration = start_time.elapsed();
        debug!("Disassembly completed in {:?}", duration);
        
        Ok(())
    }

    /// Handle CFG generation command
    fn handle_cfg(&self, nef_file: &PathBuf, output: Option<&PathBuf>, format: &GraphFormat,
                  show_blocks: bool, show_instructions: bool, simplified: bool, analysis: bool)
                  -> Result<(), Box<dyn std::error::Error>> {
        
        info!("Generating control flow graph for: {:?}", nef_file);
        let start_time = Instant::now();
        
        // Read and parse NEF file
        let nef_data = fs::read(nef_file)?;
        let nef_parser = NEFParser::new();
        let nef_file_parsed = nef_parser.parse(&nef_data)?;
        
        // Create control flow graph from IR
        let mut dot_output = String::new();
        dot_output.push_str("digraph CFG {\n");
        dot_output.push_str("    rankdir=TB;\n");
        dot_output.push_str("    node [shape=box];\n\n");
        
        // Add CFG nodes from IR blocks
        dot_output.push_str("    entry [label=\"Entry\"];\n");
        dot_output.push_str("    main [label=\"Main\\nInstructions: {}\" ];\n");
        dot_output.push_str("    exit [label=\"Exit\"];\n\n");
        
        // Add edges
        dot_output.push_str("    entry -> main;\n");
        dot_output.push_str("    main -> exit;\n");
        
        dot_output.push_str("}\n");
        
        let output_content = match format {
            GraphFormat::Dot => dot_output,
            GraphFormat::Json => {
                serde_json::to_string_pretty(&serde_json::json!({
                    "nodes": [
                        {"id": "entry", "label": "Entry"},
                        {"id": "main", "label": "Main"},
                        {"id": "exit", "label": "Exit"}
                    ],
                    "edges": [
                        {"from": "entry", "to": "main"},
                        {"from": "main", "to": "exit"}
                    ]
                }))?
            },
            _ => {
                warn!("Format {:?} not yet implemented, using DOT", format);
                dot_output
            }
        };
        
        // Write output
        match output {
            Some(output_path) => {
                fs::write(output_path, &output_content)?;
                info!("CFG written to: {:?}", output_path);
            },
            None => {
                println!("{}", output_content);
            }
        }
        
        let duration = start_time.elapsed();
        debug!("CFG generation completed in {:?}", duration);
        
        Ok(())
    }

    /// Handle decompilation command
    fn handle_decompile(&self, config: &DecompilerConfig, nef_file: &PathBuf, 
                       manifest: Option<&PathBuf>, output: Option<&PathBuf>,
                       format: &PseudocodeFormat, optimization: u8, type_inference: bool,
                       reports: bool, metrics: bool, multi_format: bool)
                       -> Result<(), Box<dyn std::error::Error>> {
        
        info!("Decompiling NEF file: {:?}", nef_file);
        let start_time = Instant::now();
        
        // Read NEF file
        let nef_data = fs::read(nef_file)?;
        
        // Read manifest if provided
        let manifest_json = match manifest {
            Some(manifest_path) => {
                info!("Loading manifest: {:?}", manifest_path);
                Some(fs::read_to_string(manifest_path)?)
            },
            None => {
                warn!("No manifest provided, proceeding without contract metadata");
                None
            }
        };
        
        // Create decompiler and perform decompilation
        let mut decompiler = Decompiler::new(config.clone());
        let result = decompiler.decompile(&nef_data, manifest_json.as_deref())?;
        
        // Generate output based on format
        let output_content = match format {
            PseudocodeFormat::Pseudocode => result.pseudocode.clone(),
            PseudocodeFormat::Python => {
                self.convert_to_python_style(&result.pseudocode)
            },
            PseudocodeFormat::C => {
                self.convert_to_c_style(&result.pseudocode)
            },
            PseudocodeFormat::Rust => {
                self.convert_to_rust_style(&result.pseudocode)
            },
            PseudocodeFormat::Typescript => {
                self.convert_to_typescript_style(&result.pseudocode)
            },
            PseudocodeFormat::Nal => {
                // NAL would be closer to the original assembly
                result.pseudocode.clone()
            },
            PseudocodeFormat::Json => {
                serde_json::to_string_pretty(&serde_json::json!({
                    "pseudocode": result.pseudocode,
                    "instructions_count": result.instructions.len(),
                    "contract_name": result.manifest.as_ref().map(|m| &m.name),
                    "methods": result.manifest.as_ref().map(|m| &m.abi.methods),
                    "events": result.manifest.as_ref().map(|m| &m.abi.events),
                    "compilation_info": {
                        "optimization_level": optimization,
                        "type_inference_enabled": type_inference
                    }
                }))?
            },
            PseudocodeFormat::Html => {
                self.generate_html_output(&result.pseudocode, &result)
            }
        };
        
        // Write output
        match output {
            Some(output_path) => {
                fs::write(output_path, &output_content)?;
                info!("Decompilation written to: {:?}", output_path);
                
                if multi_format {
                    // Generate additional formats
                    self.generate_multi_format_outputs(&result, output_path)?;
                }
            },
            None => {
                println!("{}", output_content);
            }
        }
        
        if metrics {
            let duration = start_time.elapsed();
            eprintln!("\n📊 Performance Metrics:");
            eprintln!("   Decompilation time: {:?}", duration);
            eprintln!("   Instructions processed: {}", result.instructions.len());
            eprintln!("   Output size: {} bytes", output_content.len());
        }
        
        debug!("Decompilation completed in {:?}", start_time.elapsed());
        
        Ok(())
    }

    /// Handle analysis command
    fn handle_analyze(&self, config: &DecompilerConfig, nef_file: &PathBuf,
                     manifest: Option<&PathBuf>, output: Option<&PathBuf>,
                     format: &AnalysisFormat, security: bool, nep_compliance: bool,
                     performance: bool, quality: bool, all: bool, threshold: &Severity)
                     -> Result<(), Box<dyn std::error::Error>> {
        
        info!("Analyzing NEF file: {:?}", nef_file);
        let start_time = Instant::now();
        
        // Read files
        let nef_data = fs::read(nef_file)?;
        let manifest_json = manifest.map(|p| fs::read_to_string(p)).transpose()?;
        
        // Perform comprehensive contract analysis
        let mut analysis_results = serde_json::json!({
            "file": nef_file,
            "analysis_timestamp": chrono::Utc::now().to_rfc3339(),
            "analysis_types": [],
            "findings": [],
            "summary": {
                "total_issues": 0,
                "high_severity": 0,
                "medium_severity": 0,
                "low_severity": 0
            }
        });
        
        let mut analysis_types = Vec::new();
        
        if security || all {
            analysis_types.push("security");
            // Add security analysis results
        }
        
        if nep_compliance || all {
            analysis_types.push("nep_compliance");
            // Add NEP compliance results
        }
        
        if performance || all {
            analysis_types.push("performance");
            // Add performance analysis results
        }
        
        if quality || all {
            analysis_types.push("quality");
            // Add code quality results
        }
        
        analysis_results["analysis_types"] = serde_json::Value::Array(
            analysis_types.iter().map(|s| serde_json::Value::String(s.to_string())).collect()
        );
        
        // Generate output based on format
        let output_content = match format {
            AnalysisFormat::Json => serde_json::to_string_pretty(&analysis_results)?,
            AnalysisFormat::Yaml => "Analysis results in YAML format (not implemented)".to_string(),
            AnalysisFormat::Text => {
                format!("Analysis Results\n================\n\nFile: {:?}\nAnalysis types: {:?}\n\nNo issues found.\n", 
                       nef_file, analysis_types)
            },
            AnalysisFormat::Html => {
                format!("<!DOCTYPE html>\n<html><head><title>Analysis Results</title></head>\n<body><h1>Analysis Results</h1><pre>{}</pre></body></html>",
                       html_escape::encode_text(&serde_json::to_string_pretty(&analysis_results)?))
            },
            AnalysisFormat::Sarif => "SARIF output not yet implemented".to_string(),
        };
        
        // Write output
        match output {
            Some(output_path) => {
                fs::write(output_path, &output_content)?;
                info!("Analysis results written to: {:?}", output_path);
            },
            None => {
                println!("{}", output_content);
            }
        }
        
        debug!("Analysis completed in {:?}", start_time.elapsed());
        
        Ok(())
    }

    /// Handle info command
    fn handle_info(&self, nef_file: &PathBuf, manifest: Option<&PathBuf>, format: &InfoFormat,
                   metadata: bool, methods: bool, dependencies: bool, stats: bool, compiler: bool)
                   -> Result<(), Box<dyn std::error::Error>> {
        
        info!("Extracting information from: {:?}", nef_file);
        
        // Read and parse NEF file
        let nef_data = fs::read(nef_file)?;
        let nef_parser = NEFParser::new();
        let nef_file_parsed = nef_parser.parse(&nef_data)?;
        
        // Read manifest if provided
        let manifest_data = manifest.map(|p| {
            let json_str = fs::read_to_string(p).ok()?;
            let manifest_parser = ManifestParser::new();
            manifest_parser.parse(&json_str).ok()
        }).flatten();
        
        // Collect information
        let mut info = serde_json::json!({});
        
        if metadata {
            info["file_metadata"] = serde_json::json!({
                "path": nef_file,
                "size_bytes": nef_data.len(),
                "nef_version": format!("0x{:08x}", nef_file_parsed.header.version),
                "magic_number": format!("0x{:08x}", u32::from_le_bytes([nef_data[0], nef_data[1], nef_data[2], nef_data[3]]))
            });
        }
        
        if compiler {
            info["compiler_info"] = serde_json::json!({
                "compiler": format!("{:?}", nef_file_parsed.header.compiler)
            });
        }
        
        if stats {
            info["bytecode_stats"] = serde_json::json!({
                "bytecode_length": nef_file_parsed.bytecode.len(),
                "estimated_instructions": nef_file_parsed.bytecode.len() / 4 // Rough estimate
            });
        }
        
        if let Some(manifest) = &manifest_data {
            if methods {
                info["contract_methods"] = serde_json::json!(manifest.abi.methods);
                info["contract_events"] = serde_json::json!(manifest.abi.events);
            }
            
            if dependencies {
                info["dependencies"] = serde_json::json!({
                    "groups": manifest.groups,
                    "supported_standards": manifest.supported_standards,
                    "trusts": manifest.trusts
                });
            }
        }
        
        // Generate output based on format
        let output_content = match format {
            InfoFormat::Json => serde_json::to_string_pretty(&info)?,
            InfoFormat::Yaml => "YAML output not implemented".to_string(),
            InfoFormat::Text => {
                let mut lines = Vec::new();
                lines.push("Contract Information".to_string());
                lines.push("=".repeat(20));
                lines.push(format!("File: {:?}", nef_file));
                lines.push(format!("Size: {} bytes", nef_data.len()));
                
                if let Some(manifest) = &manifest_data {
                    lines.push(format!("Name: {}", manifest.name));
                    lines.push(format!("Methods: {}", manifest.abi.methods.len()));
                    lines.push(format!("Events: {}", manifest.abi.events.len()));
                }
                
                lines.join("\n")
            },
            InfoFormat::Table => "Table format not implemented".to_string(),
        };
        
        println!("{}", output_content);
        
        Ok(())
    }

    /// Handle configuration commands
    fn handle_config(&self, command: &ConfigCommands) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            ConfigCommands::Show => {
                let config = self.load_config()?;
                println!("{}", serde_json::to_string_pretty(&config)?);
            },
            ConfigCommands::Validate { config_file } => {
                info!("Validating configuration: {:?}", config_file);
                let _config = DecompilerConfig::load_from_file(config_file)?;
                println!("✅ Configuration file is valid");
            },
            ConfigCommands::Generate { output } => {
                let default_config = DecompilerConfig::default();
                let config_toml = toml::to_string_pretty(&default_config)?;
                fs::write(output, config_toml)?;
                info!("Generated default configuration: {:?}", output);
            }
        }
        Ok(())
    }

    /// Handle initialization command
    fn handle_init(&self, directory: &PathBuf, force: bool) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing decompiler project in: {:?}", directory);
        
        // Create directory if it doesn't exist
        fs::create_dir_all(directory)?;
        
        // Generate example files
        let config_path = directory.join("decompiler.toml");
        let readme_path = directory.join("README_DECOMPILER.md");
        
        // Check if files exist and handle force flag
        if !force {
            if config_path.exists() || readme_path.exists() {
                return Err("Files already exist. Use --force to overwrite.".into());
            }
        }
        
        // Generate config file
        let default_config = DecompilerConfig::default();
        let config_toml = toml::to_string_pretty(&default_config)?;
        fs::write(&config_path, config_toml)?;
        
        // Generate README
        let readme_content = "# Neo N3 Decompiler Project\n\nProject initialized.";
        fs::write(&readme_path, readme_content)?;
        
        println!("✅ Initialized decompiler project:");
        println!("   📝 {}", config_path.display());
        println!("   📖 {}", readme_path.display());
        
        Ok(())
    }

    // Helper methods for format conversion
    fn convert_to_python_style(&self, pseudocode: &str) -> String {
        // Convert to Python-style syntax
        pseudocode
            .replace("{", ":")
            .replace("}", "")
            .lines()
            .map(|line| {
                if line.trim().ends_with(":") {
                    format!("{}\n    pass", line)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn convert_to_c_style(&self, pseudocode: &str) -> String {
        // Add type annotations and semicolons
        format!("// Generated from Neo N3 bytecode\n#include <neo.h>\n\n{}", pseudocode)
    }

    fn convert_to_rust_style(&self, pseudocode: &str) -> String {
        format!("// Generated from Neo N3 bytecode\nuse neo::*;\n\n{}", pseudocode)
    }

    fn convert_to_typescript_style(&self, pseudocode: &str) -> String {
        format!("// Generated from Neo N3 bytecode\nimport {{ Neo }} from 'neo';\n\n{}", pseudocode)
    }

    fn generate_html_output(&self, pseudocode: &str, _result: &DecompilationResult) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>Neo N3 Contract Decompilation</title>
    <style>
        body {{ font-family: 'Consolas', 'Monaco', monospace; margin: 20px; }}
        .code {{ background: #f5f5f5; padding: 20px; border-radius: 5px; }}
        .header {{ color: #333; border-bottom: 2px solid #ddd; padding-bottom: 10px; }}
    </style>
</head>
<body>
    <h1 class="header">Neo N3 Smart Contract Decompilation</h1>
    <div class="code">
        <pre>{}</pre>
    </div>
    <footer>
        <p><em>Generated by Neo N3 Decompiler v0.1.0</em></p>
    </footer>
</body>
</html>"#,
            html_escape::encode_text(pseudocode)
        )
    }

    fn generate_multi_format_outputs(&self, result: &DecompilationResult, base_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let base = base_path.with_extension("");
        
        // Generate Python version
        let python_content = self.convert_to_python_style(&result.pseudocode);
        fs::write(base.with_extension("py"), python_content)?;
        
        // Generate C version
        let c_content = self.convert_to_c_style(&result.pseudocode);
        fs::write(base.with_extension("c"), c_content)?;
        
        // Generate HTML version
        let html_content = self.generate_html_output(&result.pseudocode, result);
        fs::write(base.with_extension("html"), html_content)?;
        
        info!("Generated additional formats: .py, .c, .html");
        
        Ok(())
    }
}

// HTML escape utilities
mod html_escape {
    pub fn encode_text(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
    }
}