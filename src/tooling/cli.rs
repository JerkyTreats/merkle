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
use crate::views::OrderingPolicy;
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
}

/// CLI context for managing workspace state
pub struct CliContext {
    api: Arc<ContextApi>,
    workspace_root: PathBuf,
    config_path: Option<PathBuf>,
}

impl CliContext {
    /// Create a new CLI context
    pub fn new(workspace_root: PathBuf, config_path: Option<PathBuf>) -> Result<Self, ApiError> {
        // Initialize storage
        let store_path = workspace_root.join(".merkle").join("store");
        std::fs::create_dir_all(&store_path).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(e))
        })?;

        let node_store = Arc::new(
            SledNodeRecordStore::new(&store_path)
                .map_err(|e| ApiError::StorageError(crate::error::StorageError::IoError(
                    std::io::Error::new(std::io::ErrorKind::Other, format!("{}", e))
                )))?
        );
        let frame_storage_path = workspace_root.join(".merkle").join("frames");
        std::fs::create_dir_all(&frame_storage_path).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(e))
        })?;
        let frame_storage = Arc::new(
            crate::frame::storage::FrameStorage::new(&frame_storage_path)
                .map_err(|e| ApiError::StorageError(e))?
        );
        let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
        let basis_index = Arc::new(parking_lot::RwLock::new(BasisIndex::new()));
        let agent_registry = Arc::new(parking_lot::RwLock::new(crate::agent::AgentRegistry::new()));
        let lock_manager = Arc::new(crate::concurrency::NodeLockManager::new());

        let api = ContextApi::new(
            node_store,
            frame_storage,
            head_index,
            basis_index,
            agent_registry,
            lock_manager,
        );

        Ok(Self {
            api: Arc::new(api),
            workspace_root,
            config_path,
        })
    }

    /// Execute a CLI command
    pub fn execute(&self, command: &Commands) -> Result<String, ApiError> {
        match command {
            Commands::GetNode { node_id, max_frames } => {
                let node_id = parse_node_id(node_id)?;
                let view = ContextView {
                    max_frames: *max_frames,
                    ordering: OrderingPolicy::Recency,
                    filters: vec![],
                };
                let context = self.api.get_node(node_id, view)?;
                Ok(format!(
                    "Node: {}\nFrames: {}/{}\nPath: {}",
                    hex::encode(context.node_id),
                    context.frames.len(),
                    context.frame_count,
                    context.node_record.path.display()
                ))
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
                let frame_storage_path = self.workspace_root.join(".merkle").join("frames");
                let mut frame_count = 0;
                if frame_storage_path.exists() {
                    frame_count = count_frame_files(&frame_storage_path)?;
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
                let frame_storage_path = self.workspace_root.join(".merkle").join("frames");
                let frame_count = if frame_storage_path.exists() {
                    count_frame_files(&frame_storage_path)?
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
            Commands::ValidateProviders { agent_id } => {
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

                // Determine which agents to validate
                let agents_to_validate: Vec<_> = if let Some(ref agent_id) = agent_id {
                    config.agents.iter()
                        .filter(|(_, agent)| agent.agent_id == *agent_id)
                        .collect()
                } else {
                    config.agents.iter()
                        .filter(|(_, agent)| agent.provider_name.is_some())
                        .collect()
                };

                if agents_to_validate.is_empty() {
                    return Ok("No agents with providers found to validate".to_string());
                }

                // Validate each agent's provider
                for (_, agent_config) in agents_to_validate {
                    if let Some(provider_name) = &agent_config.provider_name {
                        match config.providers.get(provider_name) {
                            Some(provider_config) => {
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
                                                                "✓ Agent '{}': Model '{}' is available",
                                                                agent_config.agent_id,
                                                                provider_config.model
                                                            ));
                                                        } else {
                                                            errors.push(format!(
                                                                "✗ Agent '{}': Model '{}' not found. Available models: {}",
                                                                agent_config.agent_id,
                                                                provider_config.model,
                                                                available_models.join(", ")
                                                            ));
                                                        }
                                                    }
                                                    Err(e) => {
                                                        // If we can't list models, try a test completion
                                                        // For now, just report that we couldn't verify
                                                        results.push(format!(
                                                            "? Agent '{}': Model '{}' - Could not verify ({}). Model may still be valid.",
                                                            agent_config.agent_id,
                                                            provider_config.model,
                                                            e
                                                        ));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                errors.push(format!(
                                                    "✗ Agent '{}': Failed to create provider client: {}",
                                                    agent_config.agent_id,
                                                    e
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        errors.push(format!(
                                            "✗ Agent '{}': Failed to create model provider: {}",
                                            agent_config.agent_id,
                                            e
                                        ));
                                    }
                                }
                            }
                            None => {
                                errors.push(format!(
                                    "✗ Agent '{}': Provider '{}' not found in configuration",
                                    agent_config.agent_id,
                                    provider_name
                                ));
                            }
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

                // Build watch config
                let mut config = WatchConfig::default();
                config.workspace_root = self.workspace_root.clone();
                config.debounce_ms = *debounce_ms;
                config.batch_window_ms = *batch_window_ms;
                config.recursive = *recursive;
                config.max_depth = *max_depth;
                config.agent_id = agent_id.clone();
                if !ignore.is_empty() {
                    config.ignore_patterns.extend(ignore.iter().cloned());
                }

                // Create watch daemon
                let daemon = WatchDaemon::new(self.api.clone(), config);

                // Start daemon (this will block)
                info!("Starting watch mode daemon");
                daemon.start()?;

                Ok("Watch daemon stopped".to_string())
            }
        }
    }
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
