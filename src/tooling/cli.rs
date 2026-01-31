//! CLI Tooling
//!
//! Command-line interface for all Merkle operations. Provides workspace-scoped
//! operations with idempotent execution.

use crate::api::{ContextApi, ContextView};
use crate::config::ConfigLoader;
use crate::error::ApiError;
use crate::frame::{Basis, Frame};
use crate::heads::HeadIndex;
use crate::regeneration::BasisIndex;
use crate::provider::ProviderFactory;
use crate::store::{NodeRecord, NodeRecordStore};
use crate::store::persistence::SledNodeRecordStore;
use crate::tree::builder::TreeBuilder;
use crate::types::{Hash, NodeID};
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

use hex;

/// Merkle CLI - Deterministic filesystem state management
#[derive(Parser)]
#[command(name = "merkle")]
#[command(about = "Deterministic filesystem state management using Merkle trees")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Workspace root directory
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    /// Configuration file path (overrides default config loading)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error, off)
    #[arg(long)]
    pub log_level: Option<String>,

    /// Log format (json, text)
    #[arg(long)]
    pub log_format: Option<String>,

    /// Log output (stdout, stderr, file, both)
    #[arg(long)]
    pub log_output: Option<String>,

    /// Log file path (if output includes "file")
    #[arg(long)]
    pub log_file: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Get node context
    GetNode {
        /// Node ID (hex string)
        node_id: String,
        /// Maximum frames to return
        #[arg(long, default_value = "10")]
        max_frames: usize,
        /// Filter by frame type
        #[arg(long)]
        frame_type: Option<String>,
        /// Filter by agent ID
        #[arg(long)]
        agent_id: Option<String>,
        /// Show text content of frames
        #[arg(long)]
        show_content: bool,
        /// Combine all frame content with separator
        #[arg(long)]
        combine: bool,
    },
    /// Get combined text content from node context
    GetText {
        /// Node ID (hex string)
        node_id: String,
        /// Separator between frames (default: "\n\n---\n\n")
        #[arg(long, default_value = "\n\n---\n\n")]
        separator: String,
        /// Maximum frames to return
        #[arg(long, default_value = "10")]
        max_frames: usize,
        /// Filter by frame type
        #[arg(long)]
        frame_type: Option<String>,
        /// Filter by agent ID
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// Put a frame to a node
    PutFrame {
        /// Node ID (hex string)
        node_id: String,
        /// Frame content file path
        frame_file: PathBuf,
        /// Frame type
        #[arg(long)]
        frame_type: String,
        /// Agent ID
        #[arg(long)]
        agent_id: String,
    },
    /// Synthesize branch context
    Synthesize {
        /// Node ID (hex string)
        node_id: String,
        /// Frame type
        #[arg(long)]
        frame_type: String,
        /// Agent ID
        #[arg(long)]
        agent_id: String,
    },
    /// Regenerate frames for a node
    Regenerate {
        /// Node ID (hex string)
        node_id: String,
        /// Recursive regeneration
        #[arg(long)]
        recursive: bool,
        /// Agent ID
        #[arg(long)]
        agent_id: String,
    },
    /// List frames for a node
    ListFrames {
        /// Node ID (hex string)
        node_id: String,
        /// Filter by frame type
        #[arg(long)]
        frame_type: Option<String>,
    },
    /// Get head frame for a node
    GetHead {
        /// Node ID (hex string)
        node_id: String,
        /// Frame type
        #[arg(long)]
        frame_type: Option<String>,
    },
    /// Scan filesystem and rebuild tree
    Scan {
        /// Force rebuild even if tree exists
        #[arg(long)]
        force: bool,
    },
    /// Show workspace status
    Status,
    /// Validate workspace integrity
    Validate,
    /// Validate provider configurations and test model availability
    ValidateProviders {
        /// Agent ID to validate (if not provided, validates all agents with providers)
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// Start watch mode daemon
    Watch {
        /// Debounce window in milliseconds
        #[arg(long, default_value = "100")]
        debounce_ms: u64,
        /// Batch window in milliseconds
        #[arg(long, default_value = "50")]
        batch_window_ms: u64,
        /// Enable recursive regeneration
        #[arg(long)]
        recursive: bool,
        /// Maximum regeneration depth
        #[arg(long, default_value = "3")]
        max_depth: usize,
        /// Agent ID for regeneration
        #[arg(long, default_value = "watch-daemon")]
        agent_id: String,
        /// Ignore pattern (can be specified multiple times)
        #[arg(long)]
        ignore: Vec<String>,
        /// Run in foreground (default: background daemon)
        #[arg(long)]
        foreground: bool,
    },
    /// Manage agents
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
}

#[derive(Subcommand)]
pub enum AgentCommands {
    /// List all agents
    List {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Filter by role (Reader, Writer, or Synthesis)
        #[arg(long)]
        role: Option<String>,
    },
    /// Show agent details
    Show {
        /// Agent ID
        agent_id: String,
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Include prompt file content in output
        #[arg(long)]
        include_prompt: bool,
    },
    /// Validate agent configuration
    Validate {
        /// Agent ID (required unless --all is used)
        #[arg(required_unless_present = "all")]
        agent_id: Option<String>,
        /// Validate all agents
        #[arg(long, conflicts_with = "agent_id")]
        all: bool,
        /// Show detailed validation results
        #[arg(long)]
        verbose: bool,
    },
    /// Create new agent
    Create {
        /// Agent ID
        agent_id: String,
        /// Agent role (Reader, Writer, or Synthesis)
        #[arg(long)]
        role: Option<String>,
        /// Path to prompt file (required for Writer/Synthesis)
        #[arg(long)]
        prompt_path: Option<String>,
        /// Use interactive mode (default)
        #[arg(long)]
        interactive: bool,
        /// Use non-interactive mode (use flags)
        #[arg(long)]
        non_interactive: bool,
    },
    /// Edit agent configuration
    Edit {
        /// Agent ID
        agent_id: String,
        /// Update prompt file path
        #[arg(long)]
        prompt_path: Option<String>,
        /// Update agent role
        #[arg(long)]
        role: Option<String>,
        /// Editor to use (default: $EDITOR)
        #[arg(long)]
        editor: Option<String>,
    },
    /// Remove agent
    Remove {
        /// Agent ID
        agent_id: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

/// CLI context for managing workspace state
pub struct CliContext {
    api: Arc<ContextApi>,
    workspace_root: PathBuf,
    config_path: Option<PathBuf>,
    #[allow(dead_code)] // May be used for debugging or future features
    store_path: PathBuf,
    frame_storage_path: PathBuf,
}

impl CliContext {
    /// Get a reference to the underlying API
    pub fn api(&self) -> &ContextApi {
        &self.api
    }

    /// Create a new CLI context
    pub fn new(workspace_root: PathBuf, config_path: Option<PathBuf>) -> Result<Self, ApiError> {
        // Load config to get storage paths
        let config = if let Some(cfg_path) = &config_path {
            crate::config::ConfigLoader::load_from_file(cfg_path)?
        } else {
            crate::config::ConfigLoader::load(&workspace_root)?
        };
        
        // Resolve storage paths (will use XDG directories for default paths)
        let (store_path, frame_storage_path) = config.system.storage.resolve_paths(&workspace_root)?;
        
        // Initialize storage
        std::fs::create_dir_all(&store_path).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(e))
        })?;

        let node_store = Arc::new(
            SledNodeRecordStore::new(&store_path)
                .map_err(|e| ApiError::StorageError(crate::error::StorageError::IoError(
                    std::io::Error::new(std::io::ErrorKind::Other, format!("{}", e))
                )))?
        );
        
        std::fs::create_dir_all(&frame_storage_path).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(e))
        })?;
        let frame_storage = Arc::new(
            crate::frame::storage::FrameStorage::new(&frame_storage_path)
                .map_err(|e| ApiError::StorageError(e))?
        );
        // Load head index from disk, or create empty if not found
        let head_index_path = HeadIndex::persistence_path(&workspace_root);
        let head_index = Arc::new(parking_lot::RwLock::new(
            HeadIndex::load_from_disk(&head_index_path)
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to load head index from disk: {}, starting with empty index", e);
                    HeadIndex::new()
                })
        ));

        // Load basis index from disk, or create empty if not found
        let basis_index_path = BasisIndex::persistence_path(&workspace_root);
        let basis_index = Arc::new(parking_lot::RwLock::new(
            BasisIndex::load_from_disk(&basis_index_path)
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to load basis index from disk: {}, starting with empty index", e);
                    BasisIndex::new()
                })
        ));
        // Load agents and providers from config.toml first, then XDG (XDG overrides)
        let mut agent_registry = crate::agent::AgentRegistry::new();
        agent_registry.load_from_config(&config)?;
        agent_registry.load_from_xdg()?;  // XDG agents override config.toml agents
        
        let mut provider_registry = crate::provider::ProviderRegistry::new();
        provider_registry.load_from_config(&config)?;
        provider_registry.load_from_xdg()?;  // XDG providers override config.toml providers
        
        let agent_registry = Arc::new(parking_lot::RwLock::new(agent_registry));
        let provider_registry = Arc::new(parking_lot::RwLock::new(provider_registry));
        let lock_manager = Arc::new(crate::concurrency::NodeLockManager::new());

        let api = ContextApi::with_workspace_root(
            node_store,
            frame_storage,
            head_index,
            basis_index,
            agent_registry,
            provider_registry,
            lock_manager,
            workspace_root.clone(),
        );

        // Store resolved storage paths for later use
        let (store_path, frame_storage_path) = config.system.storage.resolve_paths(&workspace_root)?;

        Ok(Self {
            api: Arc::new(api),
            workspace_root,
            config_path,
            store_path,
            frame_storage_path,
        })
    }

    /// Execute a CLI command
    pub fn execute(&self, command: &Commands) -> Result<String, ApiError> {
        match command {
            Commands::GetNode { node_id, max_frames, frame_type, agent_id, show_content, combine } => {
                let node_id = parse_node_id(node_id)?;
                
                // Use builder pattern for view construction
                let mut builder = ContextView::builder().max_frames(*max_frames).recent();
                if let Some(ft) = frame_type {
                    builder = builder.by_type(ft);
                }
                if let Some(aid) = agent_id {
                    builder = builder.by_agent(aid);
                }
                let view = builder.build();
                
                let context = self.api.get_node(node_id, view)?;
                
                let mut output = format!(
                    "Node: {}\nFrames: {}/{}\nPath: {}",
                    hex::encode(context.node_id),
                    context.frames.len(),
                    context.frame_count,
                    context.node_record.path.display()
                );
                
                if *show_content || *combine {
                    if *combine {
                        // Use combined_text convenience method
                        let combined = context.combined_text("\n\n---\n\n");
                        output.push_str(&format!("\n\nCombined Content:\n{}", combined));
                    } else {
                        // Show individual frame contents
                        output.push_str("\n\nFrame Contents:");
                        for (i, frame) in context.frames.iter().enumerate() {
                            output.push_str(&format!("\n\nFrame {} (type: {}, agent: {}):", 
                                i + 1,
                                frame.frame_type,
                                frame.agent_id().unwrap_or("unknown")
                            ));
                            if let Ok(text) = frame.text_content() {
                                output.push_str(&format!("\n{}", text));
                            } else {
                                output.push_str("\n[Binary content - not UTF-8]");
                            }
                        }
                    }
                } else {
                    // Show frame summary
                    output.push_str("\n\nFrames:");
                    for (i, frame) in context.frames.iter().enumerate() {
                        output.push_str(&format!("\n  {}: {} (agent: {})", 
                            i + 1,
                            frame.frame_type,
                            frame.agent_id().unwrap_or("unknown")
                        ));
                    }
                }
                
                Ok(output)
            }
            Commands::GetText { node_id, separator, max_frames, frame_type, agent_id } => {
                let node_id = parse_node_id(node_id)?;
                
                // Use builder pattern for view construction
                let mut builder = ContextView::builder().max_frames(*max_frames).recent();
                if let Some(ft) = frame_type {
                    builder = builder.by_type(ft);
                }
                if let Some(aid) = agent_id {
                    builder = builder.by_agent(aid);
                }
                let view = builder.build();
                
                // Use convenience method for combined text
                let combined = self.api.combined_context_text(node_id, separator, view)?;
                Ok(combined)
            }
            Commands::PutFrame { node_id, frame_file, frame_type, agent_id } => {
                let node_id = parse_node_id(node_id)?;
                let content = std::fs::read(frame_file).map_err(|e| {
                    ApiError::StorageError(crate::error::StorageError::IoError(e))
                })?;
                let basis = Basis::Node(node_id);
                let frame = Frame::new(basis, content, frame_type.clone(), agent_id.clone(), HashMap::new())?;
                let frame_id = self.api.put_frame(node_id, frame, agent_id.clone())?;
                Ok(format!("Frame created: {}", hex::encode(frame_id)))
            }
            Commands::Synthesize { node_id, frame_type, agent_id } => {
                let node_id = parse_node_id(node_id)?;
                let frame_id = self.api.synthesize_branch(node_id, frame_type.clone(), agent_id.clone(), None)?;
                Ok(format!("Synthesized frame: {}", hex::encode(frame_id)))
            }
            Commands::Regenerate { node_id, recursive, agent_id } => {
                let node_id = parse_node_id(node_id)?;
                let report = self.api.regenerate(node_id, *recursive, agent_id.clone())?;
                Ok(format!(
                    "Regenerated {} frames in {}ms",
                    report.regenerated_count,
                    report.duration_ms
                ))
            }
            Commands::ListFrames { node_id, frame_type } => {
                let node_id = parse_node_id(node_id)?;
                let frame_ids = if let Some(ft) = frame_type {
                    self.api.get_head(&node_id, ft)?
                        .into_iter()
                        .collect()
                } else {
                    self.api.get_all_heads(&node_id)
                };

                if frame_ids.is_empty() {
                    Ok("No frames found".to_string())
                } else {
                    Ok(format!("Found {} frame(s)", frame_ids.len()))
                }
            }
            Commands::GetHead { node_id, frame_type } => {
                let node_id = parse_node_id(node_id)?;
                if let Some(ft) = frame_type {
                    if let Some(frame_id) = self.api.get_head(&node_id, ft)? {
                        Ok(format!("Head frame: {}", hex::encode(frame_id)))
                    } else {
                        Ok("No head frame found".to_string())
                    }
                } else {
                    let all_heads = self.api.get_all_heads(&node_id);
                    if all_heads.is_empty() {
                        Ok("No head frames found".to_string())
                    } else {
                        Ok(format!("Found {} head frame(s)", all_heads.len()))
                    }
                }
            }
            Commands::Scan { force } => {
                // Build tree from filesystem
                let builder = TreeBuilder::new(self.workspace_root.clone());
                let tree = builder.build().map_err(|e| {
                    ApiError::StorageError(e)
                })?;

                // If force is false, check if nodes already exist
                if !force {
                    // Check if root node exists
                    if self.api.node_store().get(&tree.root_id).map_err(ApiError::from)?.is_some() {
                        let root_hex = hex::encode(tree.root_id);
                        return Ok(format!(
                            "Tree already exists (root: {}). Use --force to rebuild.",
                            root_hex
                        ));
                    }
                }

                // Populate store with all nodes from tree
                NodeRecord::populate_store_from_tree(
                    self.api.node_store().as_ref() as &dyn NodeRecordStore,
                    &tree,
                ).map_err(|e| ApiError::StorageError(e))?;

                // Flush store to ensure persistence (if it's a SledNodeRecordStore)
                // We can't easily downcast Arc<dyn Trait>, so we'll skip flush for now
                // The store will flush on drop or can be flushed manually if needed

                // Format root NodeID as hex string for easy CLI usage
                let root_hex = hex::encode(tree.root_id);
                Ok(format!(
                    "Scanned {} nodes (root: {})",
                    tree.nodes.len(),
                    root_hex
                ))
            }
            Commands::Status => {
                // Compute workspace root hash
                let builder = TreeBuilder::new(self.workspace_root.clone());
                let root_hash = builder.compute_root().map_err(|e| {
                    ApiError::StorageError(e)
                })?;

                // Count nodes - we'll approximate by checking if root exists and scanning
                // For a more accurate count, we'd need to iterate the store, but that's expensive
                // So we'll just indicate if the workspace has been scanned
                let node_count = if self.api.node_store().get(&root_hash).map_err(ApiError::from)?.is_some() {
                    "scanned"
                } else {
                    "not scanned"
                };

                // Count frames by counting .frame files
                let mut frame_count = 0;
                if self.frame_storage_path.exists() {
                    frame_count = count_frame_files(&self.frame_storage_path)?;
                }

                // Count head entries (unique (node, frame_type) pairs)
                let head_count = {
                    let head_index = self.api.head_index().read();
                    head_index.heads.len()
                };

                // Count basis index entries
                let basis_count = {
                    let basis_index = self.api.basis_index().read();
                    basis_index.len()
                };

                let root_hex = hex::encode(root_hash);
                Ok(format!(
                    "Workspace Status:\n  Root hash: {}\n  Nodes: {}\n  Frames: {}\n  Head entries: {}\n  Basis entries: {}",
                    root_hex, node_count, frame_count, head_count, basis_count
                ))
            }
            Commands::Validate => {
                let mut errors = Vec::new();
                let mut warnings = Vec::new();

                // 1. Verify workspace root can be computed
                let builder = TreeBuilder::new(self.workspace_root.clone());
                let root_hash = match builder.compute_root() {
                    Ok(hash) => hash,
                    Err(e) => {
                        errors.push(format!("Failed to compute workspace root: {}", e));
                        return Ok(format!("Validation failed:\n{}", errors.join("\n")));
                    }
                };

                // 2. Verify root node exists and is accessible
                let node_count = match self.api.node_store().get(&root_hash).map_err(ApiError::from)? {
                    Some(record) => {
                        // Verify the record is valid
                        if record.node_id != root_hash {
                            errors.push(format!(
                                "Root node record has mismatched node_id: {} vs {}",
                                hex::encode(record.node_id), hex::encode(root_hash)
                            ));
                        }
                        1 // At least root exists
                    }
                    None => {
                        warnings.push("Root node not found in store - workspace may not be scanned".to_string());
                        0
                    }
                };

                // 3. Verify head index consistency
                let head_index = self.api.head_index().read();
                for node_id in head_index.get_all_node_ids() {
                    let frame_ids = head_index.get_all_heads_for_node(&node_id);
                    for frame_id in frame_ids {
                        // Verify frame exists in storage
                        if self.api.frame_storage().get(&frame_id).map_err(ApiError::from)?.is_none() {
                            warnings.push(format!(
                                "Head frame {} for node {} not found in storage",
                                hex::encode(frame_id), hex::encode(node_id)
                            ));
                        }
                    }
                }
                drop(head_index);

                // 4. Verify basis index consistency
                let basis_index = self.api.basis_index().read();
                for (_basis_hash, frame_ids) in basis_index.iter() {
                    for frame_id in frame_ids {
                        // Verify frame exists
                        if self.api.frame_storage().get(frame_id).map_err(ApiError::from)?.is_none() {
                            warnings.push(format!(
                                "Basis index frame {} not found in storage",
                                hex::encode(frame_id)
                            ));
                        }
                    }
                }
                drop(basis_index);

                // 5. Count frames and verify they're valid
                let frame_count = if self.frame_storage_path.exists() {
                    count_frame_files(&self.frame_storage_path)?
                } else {
                    0
                };

                let root_hex = hex::encode(root_hash);
                if errors.is_empty() && warnings.is_empty() {
                    Ok(format!(
                        "Validation passed:\n  Root hash: {}\n  Nodes: {}\n  Frames: {}\n  All checks passed",
                        root_hex, node_count, frame_count
                    ))
                } else {
                    let mut result = format!(
                        "Validation completed with issues:\n  Root hash: {}\n  Nodes: {}\n  Frames: {}",
                        root_hex, node_count, frame_count
                    );
                    if !errors.is_empty() {
                        result.push_str(&format!("\n\nErrors ({}):", errors.len()));
                        for error in &errors {
                            result.push_str(&format!("\n  - {}", error));
                        }
                    }
                    if !warnings.is_empty() {
                        result.push_str(&format!("\n\nWarnings ({}):", warnings.len()));
                        for warning in &warnings {
                            result.push_str(&format!("\n  - {}", warning));
                        }
                    }
                    Ok(result)
                }
            }
            Commands::ValidateProviders { agent_id: _ } => {
                // Load configuration
                let config = if let Some(ref config_path) = self.config_path {
                    // Load from specified config file
                    ConfigLoader::load_from_file(config_path)
                        .map_err(|e| ApiError::ConfigError(format!("Failed to load config from {}: {}", config_path.display(), e)))?
                } else {
                    // Load from default locations
                    ConfigLoader::load(&self.workspace_root)
                        .map_err(|e| ApiError::ConfigError(format!("Failed to load config: {}", e)))?
                };

                // Validate configuration structure
                config.validate()
                    .map_err(|errors| {
                        let error_msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
                        ApiError::ConfigError(format!("Configuration validation failed:\n{}", error_msgs.join("\n")))
                    })?;

                let mut results = Vec::new();
                let mut errors = Vec::new();

                // Validate providers directly (providers are now independent from agents)
                if config.providers.is_empty() {
                    return Ok("No providers found to validate".to_string());
                }

                // Validate each provider
                for (provider_name, provider_config) in &config.providers {
                    // Convert to ModelProvider
                    match provider_config.to_model_provider() {
                        Ok(model_provider) => {
                            // Create provider client
                            match ProviderFactory::create_client(&model_provider) {
                                Ok(client) => {
                                    // Test model availability
                                    // Use async runtime for async operations
                                    let rt = tokio::runtime::Runtime::new()
                                        .map_err(|e| ApiError::ProviderError(format!("Failed to create runtime: {}", e)))?;
                                    match rt.block_on(client.list_models()) {
                                        Ok(available_models) => {
                                            if available_models.iter().any(|m| m == &provider_config.model) {
                                                results.push(format!(
                                                    "✓ Provider '{}': Model '{}' is available",
                                                    provider_name,
                                                    provider_config.model
                                                ));
                                            } else {
                                                errors.push(format!(
                                                    "✗ Provider '{}': Model '{}' not found. Available models: {}",
                                                    provider_name,
                                                    provider_config.model,
                                                    available_models.join(", ")
                                                ));
                                            }
                                        }
                                        Err(e) => {
                                            // If we can't list models, just report that we couldn't verify
                                            results.push(format!(
                                                "? Provider '{}': Model '{}' - Could not verify ({}). Model may still be valid.",
                                                provider_name,
                                                provider_config.model,
                                                e
                                            ));
                                        }
                                    }
                                }
                                Err(e) => {
                                    errors.push(format!(
                                        "✗ Provider '{}': Failed to create provider client: {}",
                                        provider_name,
                                        e
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!(
                                "✗ Provider '{}': Failed to create model provider: {}",
                                provider_name,
                                e
                            ));
                        }
                    }
                }

                // Format output
                let mut output = String::new();
                if !results.is_empty() {
                    output.push_str("Validation Results:\n");
                    for result in &results {
                        output.push_str(&format!("  {}\n", result));
                    }
                }
                if !errors.is_empty() {
                    output.push_str("\nErrors:\n");
                    for error in &errors {
                        output.push_str(&format!("  {}\n", error));
                    }
                }

                if errors.is_empty() {
                    output.push_str("\n✓ All provider configurations are valid");
                } else {
                    output.push_str(&format!("\n✗ Found {} error(s)", errors.len()));
                }

                Ok(output)
            }
            Commands::Agent { command } => {
                self.handle_agent_command(command)
            }
            Commands::Watch {
                debounce_ms,
                batch_window_ms,
                recursive,
                max_depth,
                agent_id,
                ignore,
                foreground: _,
            } => {
                use crate::tooling::watch::{WatchConfig, WatchDaemon};

                // Load configuration to register agents
                let config = if let Some(ref config_path) = self.config_path {
                    // Load from specified config file
                    ConfigLoader::load_from_file(config_path)
                        .map_err(|e| ApiError::ConfigError(format!("Failed to load config from {}: {}", config_path.display(), e)))?
                } else {
                    // Load from default locations
                    ConfigLoader::load(&self.workspace_root)
                        .map_err(|e| ApiError::ConfigError(format!("Failed to load config: {}", e)))?
                };

                // Load agents from config into registry
                {
                    let mut registry = self.api.agent_registry().write();
                    registry.load_from_config(&config)
                        .map_err(|e| ApiError::ConfigError(format!("Failed to load agents from config: {}", e)))?;
                }

                // Build watch config
                let mut watch_config = WatchConfig::default();
                watch_config.workspace_root = self.workspace_root.clone();
                watch_config.debounce_ms = *debounce_ms;
                watch_config.batch_window_ms = *batch_window_ms;
                watch_config.recursive = *recursive;
                watch_config.max_depth = *max_depth;
                watch_config.agent_id = agent_id.clone();
                if !ignore.is_empty() {
                    watch_config.ignore_patterns.extend(ignore.iter().cloned());
                }

                // Create watch daemon
                let daemon = WatchDaemon::new(self.api.clone(), watch_config)?;

                // Start daemon (this will block)
                info!("Starting watch mode daemon");
                daemon.start()?;

                Ok("Watch daemon stopped".to_string())
            }
        }
    }

    /// Handle agent management commands
    fn handle_agent_command(&self, command: &AgentCommands) -> Result<String, ApiError> {
        match command {
            AgentCommands::List { format, role } => {
                self.handle_agent_list(format.clone(), role.as_deref())
            }
            AgentCommands::Show { agent_id, format, include_prompt } => {
                self.handle_agent_show(agent_id, format.clone(), *include_prompt)
            }
            AgentCommands::Validate { agent_id, all, verbose } => {
                self.handle_agent_validate(agent_id.as_deref(), *all, *verbose)
            }
            AgentCommands::Create { agent_id, role, prompt_path, interactive, non_interactive } => {
                self.handle_agent_create(agent_id, role.as_deref(), prompt_path.as_deref(), *interactive, *non_interactive)
            }
            AgentCommands::Edit { agent_id, prompt_path, role, editor } => {
                self.handle_agent_edit(agent_id, prompt_path.as_deref(), role.as_deref(), editor.as_deref())
            }
            AgentCommands::Remove { agent_id, force } => {
                self.handle_agent_remove(agent_id, *force)
            }
        }
    }

    /// Handle agent list command
    fn handle_agent_list(&self, format: String, role_filter: Option<&str>) -> Result<String, ApiError> {
        let registry = self.api.agent_registry().read();
        
        // Parse role filter
        let role = if let Some(role_str) = role_filter {
            match role_str {
                "Reader" => Some(crate::agent::AgentRole::Reader),
                "Writer" => Some(crate::agent::AgentRole::Writer),
                "Synthesis" => Some(crate::agent::AgentRole::Synthesis),
                _ => {
                    return Err(ApiError::ConfigError(format!(
                        "Invalid role filter: {}. Must be Reader, Writer, or Synthesis",
                        role_str
                    )));
                }
            }
        } else {
            None
        };

        let agents = registry.list_by_role(role);
        
        match format.as_str() {
            "json" => Ok(format_agent_list_json(&agents)),
            "text" | _ => Ok(format_agent_list_text(&agents)),
        }
    }

    /// Handle agent show command
    fn handle_agent_show(&self, agent_id: &str, format: String, include_prompt: bool) -> Result<String, ApiError> {
        let registry = self.api.agent_registry().read();
        
        let agent = registry.get_or_error(agent_id)?;
        
        // Load prompt content if requested
        let prompt_content = if include_prompt {
            agent.metadata.get("system_prompt").cloned()
        } else {
            None
        };

        match format.as_str() {
            "json" => Ok(format_agent_show_json(agent, prompt_content.as_deref())),
            "text" | _ => Ok(format_agent_show_text(agent, prompt_content.as_deref())),
        }
    }

    /// Handle agent validate command
    fn handle_agent_validate(&self, agent_id: Option<&str>, all: bool, verbose: bool) -> Result<String, ApiError> {
        let registry = self.api.agent_registry().read();
        
        if all {
            // Validate all agents
            let agents = registry.list_all();
            if agents.is_empty() {
                return Ok("No agents found to validate.".to_string());
            }
            
            let mut results: Vec<(String, crate::agent::ValidationResult)> = Vec::new();
            for agent in agents {
                match registry.validate_agent(&agent.agent_id) {
                    Ok(result) => results.push((agent.agent_id.clone(), result)),
                    Err(e) => {
                        // Create a validation result with error
                        let mut error_result = crate::agent::ValidationResult::new(agent.agent_id.clone());
                        error_result.add_error(format!("Failed to validate: {}", e));
                        results.push((agent.agent_id.clone(), error_result));
                    }
                }
            }
            
            Ok(format_validation_results_all(&results, verbose))
        } else {
            // Validate single agent
            let agent_id = agent_id.ok_or_else(|| {
                ApiError::ConfigError("Agent ID required unless --all is specified".to_string())
            })?;
            let result = registry.validate_agent(agent_id)?;
            Ok(format_validation_result(&result, verbose))
        }
    }

    /// Handle agent create command
    fn handle_agent_create(
        &self,
        agent_id: &str,
        role: Option<&str>,
        prompt_path: Option<&str>,
        interactive: bool,
        non_interactive: bool,
    ) -> Result<String, ApiError> {
        // Determine mode
        let is_interactive = interactive || (!non_interactive && role.is_none());

        let (final_role, final_prompt_path) = if is_interactive {
            // Interactive mode
            self.create_agent_interactive(agent_id)?
        } else {
            // Non-interactive mode
            let role = role.ok_or_else(|| {
                ApiError::ConfigError("Role is required in non-interactive mode. Use --role <role>".to_string())
            })?;
            
            let parsed_role = match role {
                "Reader" => crate::agent::AgentRole::Reader,
                "Writer" => crate::agent::AgentRole::Writer,
                "Synthesis" => crate::agent::AgentRole::Synthesis,
                _ => {
                    return Err(ApiError::ConfigError(format!(
                        "Invalid role: {}. Must be Reader, Writer, or Synthesis",
                        role
                    )));
                }
            };

            // Prompt path required for Writer/Synthesis
            let prompt = if parsed_role != crate::agent::AgentRole::Reader {
                Some(prompt_path.ok_or_else(|| {
                    ApiError::ConfigError(
                        "Prompt path is required for Writer/Synthesis agents. Use --prompt-path <path>".to_string()
                    )
                })?.to_string())
            } else {
                None
            };

            (parsed_role, prompt)
        };

        // Create agent config
        let mut agent_config = crate::config::AgentConfig {
            agent_id: agent_id.to_string(),
            role: final_role,
            system_prompt: None,
            system_prompt_path: final_prompt_path.clone(),
            metadata: HashMap::new(),
        };

        // Add user prompt templates for Writer/Synthesis
        if final_role != crate::agent::AgentRole::Reader {
            if let Some(ref prompt_path) = final_prompt_path {
                // Add default templates if not provided
                agent_config.metadata.insert(
                    "user_prompt_file".to_string(),
                    format!("Analyze the file at {{path}} using the system prompt from {}", prompt_path),
                );
                agent_config.metadata.insert(
                    "user_prompt_directory".to_string(),
                    format!("Analyze the directory at {{path}} using the system prompt from {}", prompt_path),
                );
            }
        }

        // Save config
        crate::agent::AgentRegistry::save_agent_config(agent_id, &agent_config)?;

        // Reload registry to include new agent
        {
            let mut registry = self.api.agent_registry().write();
            registry.load_from_xdg()?;
        }

        Ok(format!(
            "Agent created: {}\nConfiguration file: {}",
            agent_id,
            crate::agent::AgentRegistry::get_agent_config_path(agent_id)?.display()
        ))
    }

    /// Interactive agent creation
    fn create_agent_interactive(&self, _agent_id: &str) -> Result<(crate::agent::AgentRole, Option<String>), ApiError> {
        use dialoguer::{Select, Input};

        // Prompt for role
        let role_selection = Select::new()
            .with_prompt("Agent role")
            .items(&["Reader", "Writer", "Synthesis"])
            .default(1)
            .interact()
            .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

        let role = match role_selection {
            0 => crate::agent::AgentRole::Reader,
            1 => crate::agent::AgentRole::Writer,
            2 => crate::agent::AgentRole::Synthesis,
            _ => unreachable!(),
        };

        // Prompt for prompt path if Writer/Synthesis
        let prompt_path = if role != crate::agent::AgentRole::Reader {
            let path: String = Input::new()
                .with_prompt("Prompt file path")
                .interact_text()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;
            Some(path)
        } else {
            None
        };

        Ok((role, prompt_path))
    }

    /// Handle agent edit command
    fn handle_agent_edit(
        &self,
        agent_id: &str,
        prompt_path: Option<&str>,
        role: Option<&str>,
        editor: Option<&str>,
    ) -> Result<String, ApiError> {
        // Check if agent exists
        {
            let registry = self.api.agent_registry().read();
            registry.get_or_error(agent_id)?;
        }

        let config_path = crate::agent::AgentRegistry::get_agent_config_path(agent_id)?;

        // If flags provided, do flag-based editing
        if prompt_path.is_some() || role.is_some() {
            // Load existing config
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| ApiError::ConfigError(format!("Failed to read config: {}", e)))?;
            
            let mut agent_config: crate::config::AgentConfig = toml::from_str(&content)
                .map_err(|e| ApiError::ConfigError(format!("Failed to parse config: {}", e)))?;

            // Update fields
            if let Some(new_prompt_path) = prompt_path {
                agent_config.system_prompt_path = Some(new_prompt_path.to_string());
            }

            if let Some(new_role_str) = role {
                let new_role = match new_role_str {
                    "Reader" => crate::agent::AgentRole::Reader,
                    "Writer" => crate::agent::AgentRole::Writer,
                    "Synthesis" => crate::agent::AgentRole::Synthesis,
                    _ => {
                        return Err(ApiError::ConfigError(format!(
                            "Invalid role: {}. Must be Reader, Writer, or Synthesis",
                            new_role_str
                        )));
                    }
                };
                agent_config.role = new_role;
            }

            // Save updated config
            crate::agent::AgentRegistry::save_agent_config(agent_id, &agent_config)?;
        } else {
            // Editor-based editing
            self.edit_agent_with_editor(agent_id, editor)?;
        }

        // Reload registry
        {
            let mut registry = self.api.agent_registry().write();
            registry.load_from_xdg()?;
        }

        Ok(format!("Agent updated: {}", agent_id))
    }

    /// Edit agent config with external editor
    fn edit_agent_with_editor(&self, agent_id: &str, editor: Option<&str>) -> Result<(), ApiError> {
        use std::process::Command;

        let config_path = crate::agent::AgentRegistry::get_agent_config_path(agent_id)?;
        
        // Load existing config
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| ApiError::ConfigError(format!("Failed to read config: {}", e)))?;

        // Create temp file in system temp directory
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!("merkle-agent-{}.toml", agent_id));
        
        std::fs::write(&temp_path, content.as_bytes())
            .map_err(|e| ApiError::ConfigError(format!("Failed to write temp file: {}", e)))?;

        // Determine editor
        let editor_cmd = if let Some(ed) = editor {
            ed.to_string()
        } else {
            std::env::var("EDITOR")
                .map_err(|_| ApiError::ConfigError(
                    "No editor specified and $EDITOR not set. Use --editor <editor>".to_string()
                ))?
        };

        // Open editor
        let status = Command::new(&editor_cmd)
            .arg(&temp_path)
            .status()
            .map_err(|e| ApiError::ConfigError(format!("Failed to open editor: {}", e)))?;

        if !status.success() {
            return Err(ApiError::ConfigError("Editor exited with non-zero status".to_string()));
        }

        // Read edited content
        let edited_content = std::fs::read_to_string(&temp_path)
            .map_err(|e| ApiError::ConfigError(format!("Failed to read edited file: {}", e)))?;

        // Parse and validate
        let agent_config: crate::config::AgentConfig = toml::from_str(&edited_content)
            .map_err(|e| ApiError::ConfigError(format!("Invalid config after editing: {}", e)))?;

        // Validate agent_id matches
        if agent_config.agent_id != agent_id {
            return Err(ApiError::ConfigError(format!(
                "Agent ID mismatch: config has '{}' but expected '{}'",
                agent_config.agent_id, agent_id
            )));
        }

        // Save
        crate::agent::AgentRegistry::save_agent_config(agent_id, &agent_config)?;

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        Ok(())
    }

    /// Handle agent remove command
    fn handle_agent_remove(&self, agent_id: &str, force: bool) -> Result<String, ApiError> {
        // Check if agent exists
        {
            let registry = self.api.agent_registry().read();
            registry.get_or_error(agent_id)?;
        }

        // Confirm removal unless --force
        if !force {
            use dialoguer::Confirm;
            let confirmed = Confirm::new()
                .with_prompt(format!("Remove agent '{}'?", agent_id))
                .interact()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;
            
            if !confirmed {
                return Ok("Removal cancelled".to_string());
            }
        }

        // Delete config file
        let config_path = crate::agent::AgentRegistry::get_agent_config_path(agent_id)?;
        crate::agent::AgentRegistry::delete_agent_config(agent_id)?;

        // Note: Agent will be removed from registry on next load_from_xdg() call
        // since the config file no longer exists

        Ok(format!("Removed agent: {}\nConfiguration file deleted: {}", agent_id, config_path.display()))
    }
}

/// Format agent list as text
fn format_agent_list_text(agents: &[&crate::agent::AgentIdentity]) -> String {
    if agents.is_empty() {
        return "No agents found.\n\nNote: Agents are provider-agnostic. Providers are selected at runtime.".to_string();
    }

    let mut output = String::from("Available Agents:\n");
    for agent in agents {
        let role_str = match agent.role {
            crate::agent::AgentRole::Reader => "Reader",
            crate::agent::AgentRole::Writer => "Writer",
            crate::agent::AgentRole::Synthesis => "Synthesis",
        };
        
        let prompt_path = agent.metadata.get("system_prompt")
            .and_then(|_| {
                // Try to get the original path from config
                let config_path = crate::agent::AgentRegistry::get_agent_config_path(&agent.agent_id).ok()?;
                let content = std::fs::read_to_string(&config_path).ok()?;
                let config: crate::config::AgentConfig = toml::from_str(&content).ok()?;
                config.system_prompt_path
            })
            .unwrap_or_else(|| "[inline prompt]".to_string());

        output.push_str(&format!("  {:<20} {:<10} {}\n", agent.agent_id, role_str, prompt_path));
    }

    output.push_str(&format!("\nTotal: {} agent(s)\n\nNote: Agents are provider-agnostic. Providers are selected at runtime.", agents.len()));
    output
}

/// Format agent list as JSON
fn format_agent_list_json(agents: &[&crate::agent::AgentIdentity]) -> String {
    use serde_json::json;

    let agent_list: Vec<_> = agents.iter().map(|agent| {
        let prompt_path = agent.metadata.get("system_prompt")
            .and_then(|_| {
                let config_path = crate::agent::AgentRegistry::get_agent_config_path(&agent.agent_id).ok()?;
                let content = std::fs::read_to_string(&config_path).ok()?;
                let config: crate::config::AgentConfig = toml::from_str(&content).ok()?;
                config.system_prompt_path
            })
            .unwrap_or_else(|| "[inline prompt]".to_string());

        json!({
            "agent_id": agent.agent_id,
            "role": match agent.role {
                crate::agent::AgentRole::Reader => "Reader",
                crate::agent::AgentRole::Writer => "Writer",
                crate::agent::AgentRole::Synthesis => "Synthesis",
            },
            "system_prompt_path": prompt_path,
        })
    }).collect();

    let result = json!({
        "agents": agent_list,
        "total": agents.len(),
    });

    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Format agent show as text
fn format_agent_show_text(agent: &crate::agent::AgentIdentity, prompt_content: Option<&str>) -> String {
    let mut output = format!("Agent: {}\n", agent.agent_id);
    output.push_str(&format!("Role: {:?}\n", agent.role));

    let prompt_path = agent.metadata.get("system_prompt")
        .and_then(|_| {
            let config_path = crate::agent::AgentRegistry::get_agent_config_path(&agent.agent_id).ok()?;
            let content = std::fs::read_to_string(&config_path).ok()?;
            let config: crate::config::AgentConfig = toml::from_str(&content).ok()?;
            config.system_prompt_path
        })
        .unwrap_or_else(|| "[inline prompt]".to_string());

    output.push_str(&format!("Prompt File: {}\n", prompt_path));

    if !agent.metadata.is_empty() {
        output.push_str("\nMetadata:\n");
        for (key, value) in &agent.metadata {
            if key != "system_prompt" {
                output.push_str(&format!("  {}: {}\n", key, value));
            }
        }
    }

    if let Some(prompt) = prompt_content {
        output.push_str("\nPrompt Content:\n");
        output.push_str(prompt);
    }

    output
}

/// Format agent show as JSON
fn format_agent_show_json(agent: &crate::agent::AgentIdentity, prompt_content: Option<&str>) -> String {
    use serde_json::json;

    let prompt_path = agent.metadata.get("system_prompt")
        .and_then(|_| {
            let config_path = crate::agent::AgentRegistry::get_agent_config_path(&agent.agent_id).ok()?;
            let content = std::fs::read_to_string(&config_path).ok()?;
            let config: crate::config::AgentConfig = toml::from_str(&content).ok()?;
            config.system_prompt_path
        })
        .unwrap_or_else(|| "[inline prompt]".to_string());

    let mut metadata = json!({});
    for (key, value) in &agent.metadata {
        if key != "system_prompt" {
            metadata[key] = json!(value);
        }
    }

    let mut result = json!({
        "agent_id": agent.agent_id,
        "role": match agent.role {
            crate::agent::AgentRole::Reader => "Reader",
            crate::agent::AgentRole::Writer => "Writer",
            crate::agent::AgentRole::Synthesis => "Synthesis",
        },
        "system_prompt_path": prompt_path,
        "metadata": metadata,
    });

    if let Some(prompt) = prompt_content {
        result["prompt_content"] = json!(prompt);
    }

    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Format validation result
fn format_validation_result(result: &crate::agent::ValidationResult, verbose: bool) -> String {
    let mut output = format!("Validating agent: {}\n\n", result.agent_id);

    if result.errors.is_empty() && result.checks.iter().all(|(_, passed)| *passed) {
        output.push_str("✓ All validation checks passed\n\n");
    } else {
        // Show checks
        for (description, passed) in &result.checks {
            if *passed {
                output.push_str(&format!("✓ {}\n", description));
            } else {
                output.push_str(&format!("✗ {}\n", description));
            }
        }

        // Show errors
        if !result.errors.is_empty() {
            output.push_str("\n");
            for error in &result.errors {
                output.push_str(&format!("✗ {}\n", error));
            }
        }

        output.push_str("\n");
    }

    if verbose {
        output.push_str(&format!("Validation summary: {}/{} checks passed\n", 
            result.passed_checks(), result.total_checks()));
        if !result.errors.is_empty() {
            output.push_str(&format!("Errors found: {}\n", result.errors.len()));
        }
    } else {
        if result.is_valid() {
            output.push_str(&format!("Validation passed: {}/{} checks\n", 
                result.passed_checks(), result.total_checks()));
        } else {
            output.push_str(&format!("Validation failed: {} error(s) found\n", result.errors.len()));
        }
    }

    output
}

/// Format multiple validation results (for --all)
fn format_validation_results_all(results: &[(String, crate::agent::ValidationResult)], verbose: bool) -> String {
    let mut output = String::from("Validating all agents:\n\n");
    
    let mut valid_count = 0;
    let mut invalid_count = 0;
    
    for (agent_id, result) in results {
        if result.is_valid() {
            valid_count += 1;
            if verbose {
                output.push_str(&format!("✓ {}: All checks passed ({}/{} checks)\n", 
                    agent_id, result.passed_checks(), result.total_checks()));
            } else {
                output.push_str(&format!("✓ {}: Valid\n", agent_id));
            }
        } else {
            invalid_count += 1;
            output.push_str(&format!("✗ {}: Validation failed\n", agent_id));
            if verbose {
                // Show details for invalid agents
                for (description, passed) in &result.checks {
                    if !passed {
                        output.push_str(&format!("  ✗ {}\n", description));
                    }
                }
                for error in &result.errors {
                    output.push_str(&format!("  ✗ {}\n", error));
                }
            }
        }
    }
    
    output.push_str(&format!("\nSummary: {} valid, {} invalid (out of {} total)\n", 
        valid_count, invalid_count, results.len()));
    
    output
}

/// Parse a hex string to NodeID
fn parse_node_id(s: &str) -> Result<NodeID, ApiError> {
    // Remove 0x prefix if present
    let s = s.strip_prefix("0x").unwrap_or(s);

    // Parse hex string to bytes
    let bytes = hex::decode(s).map_err(|e| {
        ApiError::InvalidFrame(format!("Invalid hex string: {}", e))
    })?;

    if bytes.len() != 32 {
        return Err(ApiError::InvalidFrame(format!(
            "NodeID must be 32 bytes, got {} bytes",
            bytes.len()
        )));
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(Hash::from(hash))
}

/// Count frame files in the frame storage directory
fn count_frame_files(path: &PathBuf) -> Result<usize, ApiError> {
    let mut count = 0;
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(e))
        })? {
            let entry = entry.map_err(|e| {
                ApiError::StorageError(crate::error::StorageError::IoError(e))
            })?;
            let path = entry.path();
            if path.is_dir() {
                // Recursively count in subdirectories
                count += count_frame_files(&path)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("frame") {
                count += 1;
            }
        }
    }
    Ok(count)
}
