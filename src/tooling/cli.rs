//! CLI Tooling
//!
//! Command-line interface for all Merkle operations. Provides workspace-scoped
//! operations with idempotent execution.

use crate::api::{ContextApi, ContextView};
use crate::config::ConfigLoader;
use crate::error::ApiError;
use crate::frame::{Basis, Frame, FrameGenerationQueue, GenerationConfig};
use crate::heads::HeadIndex;
use crate::regeneration::BasisIndex;
use crate::store::{NodeRecord, NodeRecordStore};
use crate::store::persistence::SledNodeRecordStore;
use crate::tooling::adapter::AgentAdapter;
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

    /// Enable verbose logging (default: off)
    #[arg(long, default_value = "false")]
    pub verbose: bool,

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
    /// Workspace commands (status, validate)
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
    /// Show workspace status
    Status,
    /// Validate workspace integrity
    Validate,
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
    /// Manage providers
    Provider {
        #[command(subcommand)]
        command: ProviderCommands,
    },
    /// Initialize default agents and prompts
    Init {
        /// Force re-initialization (overwrite existing)
        #[arg(long)]
        force: bool,
        
        /// List what would be initialized without creating
        #[arg(long)]
        list: bool,
    },
    /// Context operations (generate and retrieve frames)
    Context {
        #[command(subcommand)]
        command: ContextCommands,
    },
}

#[derive(Subcommand)]
pub enum WorkspaceCommands {
    /// Show workspace status (tree, context coverage, top paths)
    Status {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Include top-level path breakdown
        #[arg(long)]
        breakdown: bool,
    },
    /// Validate workspace integrity
    Validate,
}

#[derive(Subcommand)]
pub enum AgentCommands {
    /// Show agent status (validation and prompt path)
    Status {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },
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

#[derive(Subcommand)]
pub enum ProviderCommands {
    /// Show provider status (optional connectivity)
    Status {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Test connectivity per provider (may be slow)
        #[arg(long)]
        test_connectivity: bool,
    },
    /// List all providers
    List {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Filter by provider type (openai, anthropic, ollama, local)
        #[arg(long)]
        type_filter: Option<String>,
    },
    /// Show provider details
    Show {
        /// Provider name
        provider_name: String,
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Show API key status
        #[arg(long)]
        include_credentials: bool,
    },
    /// Validate provider configuration
    Validate {
        /// Provider name
        provider_name: String,
        /// Test provider API connectivity
        #[arg(long)]
        test_connectivity: bool,
        /// Verify model is available
        #[arg(long)]
        check_model: bool,
        /// Show detailed validation results
        #[arg(long)]
        verbose: bool,
    },
    /// Test provider connectivity
    Test {
        /// Provider name
        provider_name: String,
        /// Test specific model (overrides config)
        #[arg(long)]
        model: Option<String>,
        /// Connection timeout in seconds (default: 10)
        #[arg(long, default_value = "10")]
        timeout: u64,
    },
    /// Create new provider
    Create {
        /// Provider name
        provider_name: String,
        /// Provider type (openai, anthropic, ollama, local)
        #[arg(long)]
        type_: Option<String>,
        /// Model name
        #[arg(long)]
        model: Option<String>,
        /// Endpoint URL
        #[arg(long)]
        endpoint: Option<String>,
        /// API key
        #[arg(long)]
        api_key: Option<String>,
        /// Use interactive mode (default)
        #[arg(long)]
        interactive: bool,
        /// Use non-interactive mode (use flags)
        #[arg(long)]
        non_interactive: bool,
    },
    /// Edit provider configuration
    Edit {
        /// Provider name
        provider_name: String,
        /// Update model name
        #[arg(long)]
        model: Option<String>,
        /// Update endpoint URL
        #[arg(long)]
        endpoint: Option<String>,
        /// Update API key
        #[arg(long)]
        api_key: Option<String>,
        /// Editor to use (default: $EDITOR)
        #[arg(long)]
        editor: Option<String>,
    },
    /// Remove provider
    Remove {
        /// Provider name
        provider_name: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ContextCommands {
    /// Generate context frame for a node
    Generate {
        /// Target node by NodeID (hex string)
        #[arg(long, conflicts_with = "path")]
        node: Option<String>,
        
        /// Target node by workspace-relative or absolute path
        #[arg(long, conflicts_with = "node")]
        path: Option<PathBuf>,
        
        /// Agent to use for generation
        #[arg(long)]
        agent: Option<String>,
        
        /// Provider to use for generation (required)
        #[arg(long)]
        provider: Option<String>,
        
        /// Frame type (defaults to context-<agent_id>)
        #[arg(long)]
        frame_type: Option<String>,
        
        /// Generate even if head frame exists
        #[arg(long)]
        force: bool,
        
        /// Execute immediately (default)
        #[arg(long, conflicts_with = "async")]
        sync: bool,
        
        /// Enqueue generation with Priority::Urgent
        #[arg(long, conflicts_with = "sync")]
        r#async: bool,
    },
    /// Retrieve context frames for a node
    Get {
        /// Target node by NodeID (hex string)
        #[arg(long, conflicts_with = "path")]
        node: Option<String>,
        
        /// Target node by workspace-relative or absolute path
        #[arg(long, conflicts_with = "node")]
        path: Option<PathBuf>,
        
        /// Filter by agent ID
        #[arg(long)]
        agent: Option<String>,
        
        /// Filter by frame type
        #[arg(long)]
        frame_type: Option<String>,
        
        /// Maximum frames to return
        #[arg(long, default_value = "10")]
        max_frames: usize,
        
        /// Ordering policy: recency or deterministic
        #[arg(long, default_value = "recency")]
        ordering: String,
        
        /// Concatenate frame contents with separator
        #[arg(long)]
        combine: bool,
        
        /// Separator used with --combine
        #[arg(long, default_value = "\n\n---\n\n")]
        separator: String,
        
        /// Output format: text or json
        #[arg(long, default_value = "text")]
        format: String,
        
        /// Include metadata fields in output
        #[arg(long)]
        include_metadata: bool,
        
        /// Include frames marked deleted (tombstones)
        #[arg(long)]
        include_deleted: bool,
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
    /// Optional generation queue (initialized on demand for context generate commands)
    #[allow(dead_code)] // Queue is created on demand, not stored
    queue: Option<Arc<FrameGenerationQueue>>,
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
            queue: None, // Initialize on demand for context generate commands
        })
    }

    /// Get or create the generation queue
    ///
    /// The queue is initialized lazily when needed for context generation commands.
    /// Creates a new queue each time (it's cheap to create, workers are started on first use).
    fn get_or_create_queue(&self) -> Result<Arc<FrameGenerationQueue>, ApiError> {
        let gen_config = GenerationConfig::default();
        let queue = Arc::new(FrameGenerationQueue::new(
            Arc::clone(&self.api),
            gen_config,
        ));
        
        // Start the queue workers
        queue.start()?;
        
        Ok(queue)
    }

    /// Run workspace validation (store, head index, basis index consistency).
    fn run_workspace_validate(&self) -> Result<String, ApiError> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let builder = TreeBuilder::new(self.workspace_root.clone());
        let root_hash = match builder.compute_root() {
            Ok(hash) => hash,
            Err(e) => {
                errors.push(format!("Failed to compute workspace root: {}", e));
                return Ok(format!("Validation failed:\n{}", errors.join("\n")));
            }
        };

        let node_count = match self.api.node_store().get(&root_hash).map_err(ApiError::from)? {
            Some(record) => {
                if record.node_id != root_hash {
                    errors.push(format!(
                        "Root node record has mismatched node_id: {} vs {}",
                        hex::encode(record.node_id),
                        hex::encode(root_hash)
                    ));
                }
                1
            }
            None => {
                warnings.push("Root node not found in store - workspace may not be scanned".to_string());
                0
            }
        };

        let head_index = self.api.head_index().read();
        for node_id in head_index.get_all_node_ids() {
            let frame_ids = head_index.get_all_heads_for_node(&node_id);
            for frame_id in frame_ids {
                if self.api.frame_storage().get(&frame_id).map_err(ApiError::from)?.is_none() {
                    warnings.push(format!(
                        "Head frame {} for node {} not found in storage",
                        hex::encode(frame_id),
                        hex::encode(node_id)
                    ));
                }
            }
        }
        drop(head_index);

        let basis_index = self.api.basis_index().read();
        for (_basis_hash, frame_ids) in basis_index.iter() {
            for frame_id in frame_ids {
                if self.api.frame_storage().get(frame_id).map_err(ApiError::from)?.is_none() {
                    warnings.push(format!(
                        "Basis index frame {} not found in storage",
                        hex::encode(frame_id)
                    ));
                }
            }
        }
        drop(basis_index);

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
            Commands::Workspace { command } => match command {
                WorkspaceCommands::Status { format, breakdown } => {
                    let registry = self.api.agent_registry().read();
                    let head_index = self.api.head_index().read();
                    let status = crate::workspace_status::build_workspace_status(
                        self.api.node_store().as_ref() as &dyn NodeRecordStore,
                        &head_index,
                        &registry,
                        self.workspace_root.as_path(),
                        *breakdown,
                    )?;
                    if format == "json" {
                        serde_json::to_string_pretty(&status).map_err(|e| {
                            ApiError::StorageError(crate::error::StorageError::InvalidPath(
                                e.to_string(),
                            ))
                        })
                    } else {
                        Ok(crate::workspace_status::format_workspace_status_text(
                            &status, *breakdown,
                        ))
                    }
                }
                WorkspaceCommands::Validate => self.run_workspace_validate(),
            },
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
            Commands::Validate => self.run_workspace_validate(),
            Commands::Agent { command } => {
                self.handle_agent_command(command)
            }
            Commands::Provider { command } => {
                self.handle_provider_command(command)
            }
            Commands::Init { force, list } => {
                self.handle_init(*force, *list)
            }
            Commands::Context { command } => {
                self.handle_context_command(command)
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
            AgentCommands::Status { format } => {
                self.handle_agent_status(format.clone())
            }
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

    /// Handle agent status command
    fn handle_agent_status(&self, format: String) -> Result<String, ApiError> {
        use crate::workspace_status::{AgentStatusEntry, AgentStatusOutput, format_agent_status_text};

        let registry = self.api.agent_registry().read();
        let agents = registry.list_all();
        if agents.is_empty() {
            let empty: Vec<AgentStatusEntry> = Vec::new();
            return if format == "json" {
                Ok(serde_json::to_string_pretty(&AgentStatusOutput {
                    agents: empty,
                    total: 0,
                    valid_count: 0,
                }).map_err(|e| ApiError::StorageError(crate::error::StorageError::InvalidPath(e.to_string())))?)
            } else {
                Ok(format_agent_status_text(&empty))
            };
        }
        let mut entries: Vec<AgentStatusEntry> = Vec::new();
        for agent in agents {
            let result = match registry.validate_agent(&agent.agent_id) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let role_str = match agent.role {
                crate::agent::AgentRole::Reader => "Reader",
                crate::agent::AgentRole::Writer => "Writer",
                crate::agent::AgentRole::Synthesis => "Synthesis",
            };
            let prompt_path_exists = result.checks.iter()
                .any(|(desc, passed)| desc == "Prompt file exists" && *passed);
            entries.push(AgentStatusEntry {
                agent_id: agent.agent_id.clone(),
                role: role_str.to_string(),
                valid: result.is_valid(),
                prompt_path_exists,
            });
        }
        let valid_count = entries.iter().filter(|e| e.valid).count();
        if format == "json" {
            Ok(serde_json::to_string_pretty(&AgentStatusOutput {
                agents: entries.clone(),
                total: entries.len(),
                valid_count,
            }).map_err(|e| ApiError::StorageError(crate::error::StorageError::InvalidPath(e.to_string())))?)
        } else {
            Ok(format_agent_status_text(&entries))
        }
    }

    /// Handle provider management commands
    fn handle_provider_command(&self, command: &ProviderCommands) -> Result<String, ApiError> {
        match command {
            ProviderCommands::Status { format, test_connectivity } => {
                self.handle_provider_status(format.clone(), *test_connectivity)
            }
            ProviderCommands::List { format, type_filter } => {
                self.handle_provider_list(format.clone(), type_filter.as_deref())
            }
            ProviderCommands::Show { provider_name, format, include_credentials } => {
                self.handle_provider_show(provider_name, format.clone(), *include_credentials)
            }
            ProviderCommands::Validate { provider_name, test_connectivity, check_model, verbose } => {
                self.handle_provider_validate(provider_name, *test_connectivity, *check_model, *verbose)
            }
            ProviderCommands::Test { provider_name, model, timeout } => {
                self.handle_provider_test(provider_name, model.as_deref(), *timeout)
            }
            ProviderCommands::Create { provider_name, type_, model, endpoint, api_key, interactive, non_interactive } => {
                self.handle_provider_create(provider_name, type_.as_deref(), model.as_deref(), endpoint.as_deref(), api_key.as_deref(), *interactive, *non_interactive)
            }
            ProviderCommands::Edit { provider_name, model, endpoint, api_key, editor } => {
                self.handle_provider_edit(provider_name, model.as_deref(), endpoint.as_deref(), api_key.as_deref(), editor.as_deref())
            }
            ProviderCommands::Remove { provider_name, force } => {
                self.handle_provider_remove(provider_name, *force)
            }
        }
    }

    /// Handle provider list command
    fn handle_provider_list(&self, format: String, type_filter: Option<&str>) -> Result<String, ApiError> {
        let registry = self.api.provider_registry().read();
        
        // Parse type filter
        let provider_type = if let Some(type_str) = type_filter {
            match type_str {
                "openai" => Some(crate::config::ProviderType::OpenAI),
                "anthropic" => Some(crate::config::ProviderType::Anthropic),
                "ollama" => Some(crate::config::ProviderType::Ollama),
                "local" => Some(crate::config::ProviderType::LocalCustom),
                _ => {
                    return Err(ApiError::ConfigError(format!(
                        "Invalid type filter: {}. Must be openai, anthropic, ollama, or local",
                        type_str
                    )));
                }
            }
        } else {
            None
        };

        let providers = registry.list_by_type(provider_type);
        
        match format.as_str() {
            "json" => Ok(format_provider_list_json(&providers)),
            "text" | _ => Ok(format_provider_list_text(&providers)),
        }
    }

    /// Handle provider show command
    fn handle_provider_show(&self, provider_name: &str, format: String, include_credentials: bool) -> Result<String, ApiError> {
        let registry = self.api.provider_registry().read();
        
        let provider = registry.get_or_error(provider_name)?;
        
        // Resolve API key status
        let api_key_status = if include_credentials {
            Some(self.resolve_api_key_status(provider))
        } else {
            None
        };

        match format.as_str() {
            "json" => Ok(format_provider_show_json(provider, api_key_status.as_deref())),
            "text" | _ => Ok(format_provider_show_text(provider, api_key_status.as_deref())),
        }
    }

    /// Resolve API key status for a provider
    fn resolve_api_key_status(&self, provider: &crate::config::ProviderConfig) -> String {
        match provider.provider_type {
            crate::config::ProviderType::OpenAI => {
                if provider.api_key.is_some() {
                    "Set (from config)".to_string()
                } else if std::env::var("OPENAI_API_KEY").is_ok() {
                    "Set (from environment)".to_string()
                } else {
                    "Not set".to_string()
                }
            }
            crate::config::ProviderType::Anthropic => {
                if provider.api_key.is_some() {
                    "Set (from config)".to_string()
                } else if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                    "Set (from environment)".to_string()
                } else {
                    "Not set".to_string()
                }
            }
            crate::config::ProviderType::Ollama | crate::config::ProviderType::LocalCustom => {
                "Not required".to_string()
            }
        }
    }

    /// Handle provider validate command (single provider per provider_validate_spec)
    fn handle_provider_validate(&self, provider_name: &str, test_connectivity: bool, check_model: bool, verbose: bool) -> Result<String, ApiError> {
        let registry = self.api.provider_registry().read();
        let mut result = registry.validate_provider(provider_name)?;

        if test_connectivity || check_model {
            match registry.create_client(provider_name) {
                Ok(client) => {
                    result.add_check("Provider client created", true);
                    let rt = tokio::runtime::Runtime::new()
                        .map_err(|e| ApiError::ProviderError(format!("Failed to create runtime: {}", e)))?;
                    match rt.block_on(client.list_models()) {
                        Ok(available_models) => {
                            result.add_check("API connectivity: OK", true);
                            if check_model {
                                let provider = registry.get_or_error(provider_name)?;
                                if available_models.iter().any(|m| m == &provider.model) {
                                    result.add_check(&format!("Model '{}' is available", provider.model), true);
                                } else {
                                    result.add_error(format!(
                                        "Model '{}' not found. Available models: {}",
                                        provider.model,
                                        available_models.join(", ")
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            result.add_error(format!("API connectivity failed: {}", e));
                        }
                    }
                }
                Err(e) => {
                    result.add_error(format!("Failed to create provider client: {}", e));
                }
            }
        }

        Ok(format_provider_validation_result(&result, verbose))
    }

    /// Handle provider status command
    fn handle_provider_status(&self, format: String, test_connectivity: bool) -> Result<String, ApiError> {
        use crate::workspace_status::{ProviderStatusEntry, ProviderStatusOutput, format_provider_status_text};

        let registry = self.api.provider_registry().read();
        let providers = registry.list_all();
        if providers.is_empty() {
            let empty: Vec<ProviderStatusEntry> = Vec::new();
            return if format == "json" {
                Ok(serde_json::to_string_pretty(&ProviderStatusOutput {
                    providers: empty,
                    total: 0,
                }).map_err(|e| ApiError::StorageError(crate::error::StorageError::InvalidPath(e.to_string())))?)
            } else {
                Ok(format_provider_status_text(&empty, false))
            };
        }
        let mut entries: Vec<ProviderStatusEntry> = Vec::new();
        for provider in providers {
            let provider_name = provider.provider_name.as_deref().unwrap_or("unknown").to_string();
            let type_str = match provider.provider_type {
                crate::config::ProviderType::OpenAI => "openai",
                crate::config::ProviderType::Anthropic => "anthropic",
                crate::config::ProviderType::Ollama => "ollama",
                crate::config::ProviderType::LocalCustom => "local",
            };
            let connectivity = if test_connectivity {
                match registry.create_client(&provider_name) {
                    Ok(client) => {
                        let rt = tokio::runtime::Runtime::new()
                            .map_err(|e| ApiError::ProviderError(format!("Failed to create runtime: {}", e)))?;
                        match rt.block_on(client.list_models()) {
                            Ok(_) => Some("ok".to_string()),
                            Err(_) => Some("fail".to_string()),
                        }
                    }
                    Err(_) => Some("fail".to_string()),
                }
            } else {
                None
            };
            entries.push(ProviderStatusEntry {
                provider_name,
                provider_type: type_str.to_string(),
                model: provider.model.clone(),
                connectivity,
            });
        }
        if format == "json" {
            Ok(serde_json::to_string_pretty(&ProviderStatusOutput {
                providers: entries.clone(),
                total: entries.len(),
            }).map_err(|e| ApiError::StorageError(crate::error::StorageError::InvalidPath(e.to_string())))?)
        } else {
            Ok(format_provider_status_text(&entries, test_connectivity))
        }
    }

    /// Handle provider test command
    fn handle_provider_test(&self, provider_name: &str, model_override: Option<&str>, timeout: u64) -> Result<String, ApiError> {
        let registry = self.api.provider_registry().read();
        
        // Get provider config
        let provider = registry.get_or_error(provider_name)?;
        
        // Create client
        let client = registry.create_client(provider_name)?;
        
        let mut output = format!("Testing provider: {}\n\n", provider_name);
        
        // Test connectivity
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| ApiError::ProviderError(format!("Failed to create runtime: {}", e)))?;
        
        let start = std::time::Instant::now();
        match rt.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(timeout),
                client.list_models()
            ).await
        }) {
            Ok(Ok(available_models)) => {
                let elapsed = start.elapsed();
                output.push_str(&format!("✓ Provider client created\n"));
                output.push_str(&format!("✓ API connectivity: OK ({}ms)\n", elapsed.as_millis()));
                
                // Check model availability
                let model_to_check = model_override.unwrap_or(&provider.model);
                if available_models.iter().any(|m| m == model_to_check) {
                    output.push_str(&format!("✓ Model '{}' is available\n", model_to_check));
                } else {
                    output.push_str(&format!("✗ Model '{}' not found\n", model_to_check));
                    output.push_str(&format!("Available models: {}\n", available_models.join(", ")));
                    return Ok(output);
                }
            }
            Ok(Err(e)) => {
                output.push_str(&format!("✗ API connectivity failed: {}\n", e));
                return Ok(output);
            }
            Err(_) => {
                output.push_str(&format!("✗ API connectivity timeout ({}s)\n", timeout));
                return Ok(output);
            }
        }

        output.push_str(&format!("\nProvider is working correctly.\n"));
        Ok(output)
    }

    /// Handle provider create command
    fn handle_provider_create(
        &self,
        provider_name: &str,
        type_: Option<&str>,
        model: Option<&str>,
        endpoint: Option<&str>,
        api_key: Option<&str>,
        interactive: bool,
        non_interactive: bool,
    ) -> Result<String, ApiError> {
        // Determine mode
        let is_interactive = interactive || (!non_interactive && type_.is_none());

        let (provider_type, final_model, final_endpoint, final_api_key, default_options) = if is_interactive {
            // Interactive mode
            self.create_provider_interactive()?
        } else {
            // Non-interactive mode
            let type_str = type_.ok_or_else(|| {
                ApiError::ConfigError("Provider type is required in non-interactive mode. Use --type <type>".to_string())
            })?;
            
            let parsed_type = match type_str {
                "openai" => crate::config::ProviderType::OpenAI,
                "anthropic" => crate::config::ProviderType::Anthropic,
                "ollama" => crate::config::ProviderType::Ollama,
                "local" => crate::config::ProviderType::LocalCustom,
                _ => {
                    return Err(ApiError::ConfigError(format!(
                        "Invalid provider type: {}. Must be openai, anthropic, ollama, or local",
                        type_str
                    )));
                }
            };

            let model_name = model.ok_or_else(|| {
                ApiError::ConfigError("Model is required in non-interactive mode. Use --model <model>".to_string())
            })?;

            (parsed_type, model_name.to_string(), endpoint.map(|s| s.to_string()), api_key.map(|s| s.to_string()), crate::provider::CompletionOptions::default())
        };

        // Create provider config
        let provider_config = crate::config::ProviderConfig {
            provider_name: Some(provider_name.to_string()),
            provider_type,
            model: final_model,
            api_key: final_api_key,
            endpoint: final_endpoint,
            default_options,
        };

        // Save config
        crate::provider::ProviderRegistry::save_provider_config(provider_name, &provider_config)?;

        // Reload registry to include new provider
        {
            let mut registry = self.api.provider_registry().write();
            registry.load_from_xdg()?;
        }

        Ok(format!(
            "Provider created: {}\nConfiguration file: {}",
            provider_name,
            crate::provider::ProviderRegistry::get_provider_config_path(provider_name)?.display()
        ))
    }

    /// Interactive provider creation
    fn create_provider_interactive(&self) -> Result<(crate::config::ProviderType, String, Option<String>, Option<String>, crate::provider::CompletionOptions), ApiError> {
        use dialoguer::{Select, Input};

        // Prompt for provider type
        let type_selection = Select::new()
            .with_prompt("Provider type")
            .items(&["openai", "anthropic", "ollama", "local"])
            .default(0)
            .interact()
            .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

        let provider_type = match type_selection {
            0 => crate::config::ProviderType::OpenAI,
            1 => crate::config::ProviderType::Anthropic,
            2 => crate::config::ProviderType::Ollama,
            3 => crate::config::ProviderType::LocalCustom,
            _ => unreachable!(),
        };

        // Prompt for model name
        let model: String = Input::new()
            .with_prompt("Model name")
            .interact_text()
            .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

        // Prompt for endpoint (with defaults)
        let default_endpoint = match provider_type {
            crate::config::ProviderType::OpenAI => Some("https://api.openai.com/v1".to_string()),
            crate::config::ProviderType::Ollama => Some("http://localhost:11434".to_string()),
            crate::config::ProviderType::LocalCustom => None, // Required
            crate::config::ProviderType::Anthropic => None, // No custom endpoint needed
        };

        let endpoint = if provider_type == crate::config::ProviderType::LocalCustom {
            // Required for local
            Some(Input::new()
                .with_prompt("Endpoint URL (required)")
                .interact_text()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?)
        } else if let Some(default) = default_endpoint {
            // Optional with default
            let input: String = Input::new()
                .with_prompt(format!("Endpoint URL (optional, default: {})", default))
                .default(default)
                .interact_text()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;
            Some(input)
        } else {
            None
        };

        // Prompt for API key (optional, suggest env var)
        let env_var = match provider_type {
            crate::config::ProviderType::OpenAI => "OPENAI_API_KEY",
            crate::config::ProviderType::Anthropic => "ANTHROPIC_API_KEY",
            _ => "",
        };

        let api_key = if provider_type == crate::config::ProviderType::Ollama || provider_type == crate::config::ProviderType::LocalCustom {
            None
        } else {
            let prompt = if !env_var.is_empty() {
                format!("API key (optional, will use {} env var if not set)", env_var)
            } else {
                "API key (optional)".to_string()
            };
            
            let input: String = Input::new()
                .with_prompt(prompt)
                .allow_empty(true)
                .interact_text()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;
            
            if input.is_empty() {
                None
            } else {
                Some(input)
            }
        };

        // Prompt for default completion options
        let temperature: f32 = Input::new()
            .with_prompt("Default temperature (0.0-2.0, default: 1.0)")
            .default(1.0)
            .interact_text()
            .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

        let max_tokens: Option<u32> = {
            let input: String = Input::new()
                .with_prompt("Default max tokens (optional, press Enter to skip)")
                .allow_empty(true)
                .interact_text()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;
            
            if input.is_empty() {
                None
            } else {
                input.parse().ok()
            }
        };

        let default_options = crate::provider::CompletionOptions {
            temperature: Some(temperature),
            max_tokens,
            ..Default::default()
        };

        Ok((provider_type, model, endpoint, api_key, default_options))
    }

    /// Handle provider edit command
    fn handle_provider_edit(
        &self,
        provider_name: &str,
        model: Option<&str>,
        endpoint: Option<&str>,
        api_key: Option<&str>,
        editor: Option<&str>,
    ) -> Result<String, ApiError> {
        // Check if provider exists
        {
            let registry = self.api.provider_registry().read();
            registry.get_or_error(provider_name)?;
        }

        let config_path = crate::provider::ProviderRegistry::get_provider_config_path(provider_name)?;

        // If flags provided, do flag-based editing
        if model.is_some() || endpoint.is_some() || api_key.is_some() {
            // Load existing config
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| ApiError::ConfigError(format!("Failed to read config: {}", e)))?;
            
            let mut provider_config: crate::config::ProviderConfig = toml::from_str(&content)
                .map_err(|e| ApiError::ConfigError(format!("Failed to parse config: {}", e)))?;

            // Update fields
            if let Some(new_model) = model {
                provider_config.model = new_model.to_string();
            }

            if let Some(new_endpoint) = endpoint {
                provider_config.endpoint = Some(new_endpoint.to_string());
            }

            if let Some(new_api_key) = api_key {
                provider_config.api_key = Some(new_api_key.to_string());
            }

            // Save updated config
            crate::provider::ProviderRegistry::save_provider_config(provider_name, &provider_config)?;
        } else {
            // Editor-based editing
            self.edit_provider_with_editor(provider_name, editor)?;
        }

        // Reload registry
        {
            let mut registry = self.api.provider_registry().write();
            registry.load_from_xdg()?;
        }

        Ok(format!("Provider updated: {}", provider_name))
    }

    /// Edit provider config with external editor
    fn edit_provider_with_editor(&self, provider_name: &str, editor: Option<&str>) -> Result<(), ApiError> {
        use std::process::Command;

        let config_path = crate::provider::ProviderRegistry::get_provider_config_path(provider_name)?;
        
        // Load existing config
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| ApiError::ConfigError(format!("Failed to read config: {}", e)))?;

        // Create temp file in system temp directory
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!("merkle-provider-{}.toml", provider_name));
        
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
        let provider_config: crate::config::ProviderConfig = toml::from_str(&edited_content)
            .map_err(|e| ApiError::ConfigError(format!("Invalid config after editing: {}", e)))?;

        // Validate provider_name matches
        if let Some(ref config_name) = provider_config.provider_name {
            if config_name != provider_name {
                return Err(ApiError::ConfigError(format!(
                    "Provider name mismatch: config has '{}' but expected '{}'",
                    config_name, provider_name
                )));
            }
        }

        // Save
        crate::provider::ProviderRegistry::save_provider_config(provider_name, &provider_config)?;

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        Ok(())
    }

    /// Handle provider remove command
    fn handle_provider_remove(&self, provider_name: &str, force: bool) -> Result<String, ApiError> {
        // Check if provider exists
        {
            let registry = self.api.provider_registry().read();
            registry.get_or_error(provider_name)?;
        }

        // Check if provider might be in use (warn)
        {
            let registry = self.api.provider_registry().read();
            let provider = registry.get_or_error(provider_name)?;
            if provider.provider_type == crate::config::ProviderType::OpenAI || 
               provider.provider_type == crate::config::ProviderType::Anthropic {
                // Warn for cloud providers
                eprintln!("Warning: Provider '{}' may be in use by agents.", provider_name);
            }
        }

        // Confirm removal unless --force
        if !force {
            use dialoguer::Confirm;
            let confirmed = Confirm::new()
                .with_prompt(format!("Remove provider '{}'?", provider_name))
                .interact()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;
            
            if !confirmed {
                return Ok("Removal cancelled".to_string());
            }
        }

        // Delete config file
        let config_path = crate::provider::ProviderRegistry::get_provider_config_path(provider_name)?;
        crate::provider::ProviderRegistry::delete_provider_config(provider_name)?;

        Ok(format!("Removed provider: {}\nConfiguration file deleted: {}", provider_name, config_path.display()))
    }

    /// Handle init command
    fn handle_init(&self, force: bool, list: bool) -> Result<String, ApiError> {
        if list {
            let preview = crate::init::list_initialization()?;
            Ok(format_init_preview(&preview))
        } else {
            let summary = crate::init::initialize_all(force)?;
            Ok(format_init_summary(&summary, force))
        }
    }

    /// Handle context management commands
    fn handle_context_command(&self, command: &ContextCommands) -> Result<String, ApiError> {
        match command {
            ContextCommands::Generate {
                node,
                path,
                agent,
                provider,
                frame_type,
                force,
                sync,
                r#async,
            } => {
                self.handle_context_generate(
                    node.as_deref(),
                    path.as_ref(),
                    agent.as_deref(),
                    provider.as_deref(),
                    frame_type.as_deref(),
                    *force,
                    *sync,
                    *r#async,
                )
            }
            ContextCommands::Get {
                node,
                path,
                agent,
                frame_type,
                max_frames,
                ordering,
                combine,
                separator,
                format,
                include_metadata,
                include_deleted,
            } => {
                self.handle_context_get(
                    node.as_deref(),
                    path.as_ref(),
                    agent.as_deref(),
                    frame_type.as_deref(),
                    *max_frames,
                    ordering,
                    *combine,
                    separator,
                    format,
                    *include_metadata,
                    *include_deleted,
                )
            }
        }
    }

    /// Resolve agent ID (default to single Writer agent if not specified)
    fn resolve_agent_id(&self, agent_id: Option<&str>) -> Result<String, ApiError> {
        if let Some(agent_id) = agent_id {
            // Verify agent exists
            self.api.get_agent(agent_id)?;
            return Ok(agent_id.to_string());
        }

        // Find Writer agents
        let (agent_count, agent_ids) = {
            let registry = self.api.agent_registry().read();
            let writer_agents = registry.list_by_role(Some(crate::agent::AgentRole::Writer));
            let agent_ids: Vec<String> = writer_agents.iter().map(|a| a.agent_id.clone()).collect();
            (agent_ids.len(), agent_ids)
        };

        match agent_count {
            0 => Err(ApiError::ConfigError(
                "No Writer agents found. Use `merkle agent list` to see available agents, or use `--agent <agent_id>` to specify an agent.".to_string()
            )),
            1 => Ok(agent_ids[0].clone()),
            _ => {
                Err(ApiError::ConfigError(format!(
                    "Multiple Writer agents found: {}. Use `--agent <agent_id>` to specify which agent to use.",
                    agent_ids.join(", ")
                )))
            }
        }
    }

    /// Resolve provider name (must be specified)
    fn resolve_provider_name(&self, provider_name: Option<&str>) -> Result<String, ApiError> {
        let provider_name = provider_name.ok_or_else(|| {
            ApiError::ProviderNotConfigured(
                "Provider is required. Use `--provider <provider_name>` to specify a provider. Use `merkle provider list` to see available providers.".to_string()
            )
        })?;

        // Verify provider exists
        let registry = self.api.provider_registry().read();
        registry.get_or_error(provider_name)?;
        drop(registry);

        Ok(provider_name.to_string())
    }

    /// Handle context generate command
    fn handle_context_generate(
        &self,
        node: Option<&str>,
        path: Option<&PathBuf>,
        agent: Option<&str>,
        provider: Option<&str>,
        frame_type: Option<&str>,
        force: bool,
        _sync: bool,
        r#async: bool,
    ) -> Result<String, ApiError> {
        // 1. Path/NodeID resolution (mutually exclusive)
        let node_id = match (node, path) {
            (Some(node_str), None) => {
                // Parse NodeID
                parse_node_id(node_str)?
            }
            (None, Some(path)) => {
                // Resolve path to NodeID
                resolve_path_to_node_id(&self.api, path, &self.workspace_root)?
            }
            (Some(_), Some(_)) => {
                return Err(ApiError::ConfigError(
                    "Cannot specify both --node and --path. Use one or the other.".to_string()
                ));
            }
            (None, None) => {
                return Err(ApiError::ConfigError(
                    "Must specify either --node <node_id> or --path <path>.".to_string()
                ));
            }
        };

        // 2. Agent resolution
        let agent_id = self.resolve_agent_id(agent)?;

        // 3. Provider resolution
        let provider_name = self.resolve_provider_name(provider)?;

        // 4. Frame type resolution
        let frame_type = frame_type.map(|s| s.to_string())
            .unwrap_or_else(|| format!("context-{}", agent_id));

        // 5. Agent validation
        let agent = self.api.get_agent(&agent_id)?;
        
        // Verify agent has Writer or Synthesis role
        if agent.role != crate::agent::AgentRole::Writer && agent.role != crate::agent::AgentRole::Synthesis {
            return Err(ApiError::Unauthorized(format!(
                "Agent '{}' has role {:?}, but only Writer or Synthesis agents can generate frames.",
                agent_id, agent.role
            )));
        }

        // Verify node exists
        let _node_record = self.api.node_store().get(&node_id).map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;
        
        // Check if agent has system_prompt in metadata
        if !agent.metadata.contains_key("system_prompt") {
            return Err(ApiError::ConfigError(format!(
                "Agent '{}' is missing system_prompt. Use `merkle agent validate {}` to check agent configuration.",
                agent_id, agent_id
            )));
        }

        // 6. Head frame check (unless --force)
        if !force {
            if let Some(head_frame_id) = self.api.get_head(&node_id, &frame_type)? {
                return Ok(format!(
                    "Frame already exists: {}\nUse --force to generate a new frame.",
                    hex::encode(head_frame_id)
                ));
            }
        }

        // 7. Generation (sync or async)
        // Default is sync unless --async is explicitly specified
        let is_async = r#async;
        
        // Create runtime before calling get_or_create_queue() which needs it for queue.start()
        // Check if we're already in a runtime (shouldn't happen in CLI, but can in tests)
        let rt = if let Ok(_handle) = tokio::runtime::Handle::try_current() {
            // We're in a runtime - can't create another one or use block_on
            // This should not happen in normal CLI usage, but can occur in tests
            // For now, return an error - the caller should handle this case
            return Err(ApiError::ProviderError(
                "Cannot generate context from within an async runtime context. This is a limitation when running from async tests.".to_string()
            ));
        } else {
            // No runtime exists, create one
            tokio::runtime::Runtime::new()
                .map_err(|e| ApiError::ProviderError(format!("Failed to create runtime: {}", e)))?
        };
        
        // Enter runtime context for queue.start() which needs tokio::spawn
        let _guard = rt.enter();
        let queue = self.get_or_create_queue()?;
        // Drop guard before using block_on (can't block while in runtime context)
        drop(_guard);
        
        if is_async {
            // Async mode: enqueue and return immediately
            let request_id = rt.block_on(async {
                queue.enqueue(
                    node_id,
                    agent_id.clone(),
                    provider_name.clone(),
                    Some(frame_type.clone()),
                    crate::frame::queue::Priority::Urgent,
                ).await
            })?;
            
            Ok(format!("Generation enqueued: request_id={:?}", request_id))
        } else {
            // Sync mode: enqueue and wait for completion
            let adapter = crate::tooling::adapter::ContextApiAdapter::with_queue(
                Arc::clone(&self.api),
                queue,
            );
            
            // Create a dummy prompt (queue will generate the actual prompt from agent metadata)
            let dummy_prompt = String::new();
            
            let frame_id = rt.block_on(async {
                adapter.generate_frame(
                    node_id,
                    dummy_prompt,
                    frame_type.clone(),
                    agent_id.clone(),
                    provider_name.clone(),
                ).await
            })?;
            
            Ok(format!("Frame generated: {}", hex::encode(frame_id)))
        }
    }

    /// Handle context get command
    fn handle_context_get(
        &self,
        node: Option<&str>,
        path: Option<&PathBuf>,
        agent: Option<&str>,
        frame_type: Option<&str>,
        max_frames: usize,
        ordering: &str,
        combine: bool,
        separator: &str,
        format: &str,
        include_metadata: bool,
        include_deleted: bool,
    ) -> Result<String, ApiError> {
        // 1. Path/NodeID resolution
        let node_id = match (node, path) {
            (Some(node_str), None) => {
                parse_node_id(node_str)?
            }
            (None, Some(path)) => {
                resolve_path_to_node_id(&self.api, path, &self.workspace_root)?
            }
            (Some(_), Some(_)) => {
                return Err(ApiError::ConfigError(
                    "Cannot specify both --node and --path. Use one or the other.".to_string()
                ));
            }
            (None, None) => {
                return Err(ApiError::ConfigError(
                    "Must specify either --node <node_id> or --path <path>.".to_string()
                ));
            }
        };

        // 2. Build ContextView
        let ordering_policy = match ordering {
            "recency" => crate::views::OrderingPolicy::Recency,
            "deterministic" => crate::views::OrderingPolicy::Type, // Use type ordering for deterministic
            _ => {
                return Err(ApiError::ConfigError(format!(
                    "Invalid ordering: '{}'. Must be 'recency' or 'deterministic'.",
                    ordering
                )));
            }
        };

        let mut builder = ContextView::builder()
            .max_frames(max_frames);

        // Set ordering
        match ordering_policy {
            crate::views::OrderingPolicy::Recency => {
                builder = builder.recent();
            }
            crate::views::OrderingPolicy::Type => {
                builder = builder.by_type_ordering(); // Deterministic ordering by type
            }
            _ => {
                builder = builder.recent(); // Default to recency
            }
        }

        // Add filters
        if let Some(agent_id) = agent {
            builder = builder.by_agent(agent_id);
        }
        if let Some(ft) = frame_type {
            builder = builder.by_type(ft);
        }
        if !include_deleted {
            // Exclude deleted frames by default
            // Note: FrameFilter::ExcludeDeleted would need to be added to views.rs
            // For now, we'll filter in the output formatting
        }

        let view = builder.build();

        // 3. Retrieve context
        let context = self.api.get_node(node_id, view)?;

        // 4. Format output
        match format {
            "text" => {
                format_context_text_output(&context, include_metadata, combine, separator, include_deleted)
            }
            "json" => {
                format_context_json_output(&context, include_metadata, include_deleted)
            }
            _ => {
                Err(ApiError::ConfigError(format!(
                    "Invalid format: '{}'. Must be 'text' or 'json'.",
                    format
                )))
            }
        }
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

/// Format provider list as text
fn format_provider_list_text(providers: &[&crate::config::ProviderConfig]) -> String {
    if providers.is_empty() {
        return "No providers found.\n\nUse 'merkle provider create' to add a provider.".to_string();
    }

    let mut output = String::from("Available Providers:\n");
    for provider in providers {
        let type_str = match provider.provider_type {
            crate::config::ProviderType::OpenAI => "openai",
            crate::config::ProviderType::Anthropic => "anthropic",
            crate::config::ProviderType::Ollama => "ollama",
            crate::config::ProviderType::LocalCustom => "local",
        };
        
        let endpoint_str = provider.endpoint.as_deref().unwrap_or("(default endpoint)");
        let provider_name = provider.provider_name.as_deref().unwrap_or("unknown");
        
        output.push_str(&format!("  {:<20} {:<10} {:<20} {}\n", 
            provider_name, type_str, provider.model, endpoint_str));
    }

    output.push_str(&format!("\nTotal: {} provider(s)\n", providers.len()));
    output
}

/// Format provider list as JSON
fn format_provider_list_json(providers: &[&crate::config::ProviderConfig]) -> String {
    use serde_json::json;

    let provider_list: Vec<_> = providers.iter().map(|provider| {
        let type_str = match provider.provider_type {
            crate::config::ProviderType::OpenAI => "openai",
            crate::config::ProviderType::Anthropic => "anthropic",
            crate::config::ProviderType::Ollama => "ollama",
            crate::config::ProviderType::LocalCustom => "local",
        };
        
        json!({
            "provider_name": provider.provider_name.as_deref().unwrap_or("unknown"),
            "provider_type": type_str,
            "model": provider.model,
            "endpoint": provider.endpoint,
        })
    }).collect();

    let result = json!({
        "providers": provider_list,
        "total": providers.len()
    });

    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Format provider show as text
fn format_provider_show_text(provider: &crate::config::ProviderConfig, api_key_status: Option<&str>) -> String {
    let mut output = format!("Provider: {}\n", provider.provider_name.as_deref().unwrap_or("unknown"));
    
    let type_str = match provider.provider_type {
        crate::config::ProviderType::OpenAI => "openai",
        crate::config::ProviderType::Anthropic => "anthropic",
        crate::config::ProviderType::Ollama => "ollama",
        crate::config::ProviderType::LocalCustom => "local",
    };
    output.push_str(&format!("Type: {}\n", type_str));
    output.push_str(&format!("Model: {}\n", provider.model));
    
    if let Some(endpoint) = &provider.endpoint {
        output.push_str(&format!("Endpoint: {}\n", endpoint));
    } else {
        output.push_str("Endpoint: (default endpoint)\n");
    }

    if let Some(status) = api_key_status {
        output.push_str(&format!("API Key: {}\n", status));
    }

    output.push_str("\nDefault Completion Options:\n");
    if let Some(temp) = provider.default_options.temperature {
        output.push_str(&format!("  temperature: {}\n", temp));
    }
    if let Some(max_tokens) = provider.default_options.max_tokens {
        output.push_str(&format!("  max_tokens: {}\n", max_tokens));
    }
    if let Some(top_p) = provider.default_options.top_p {
        output.push_str(&format!("  top_p: {}\n", top_p));
    }
    if let Some(freq_penalty) = provider.default_options.frequency_penalty {
        output.push_str(&format!("  frequency_penalty: {}\n", freq_penalty));
    }
    if let Some(pres_penalty) = provider.default_options.presence_penalty {
        output.push_str(&format!("  presence_penalty: {}\n", pres_penalty));
    }
    if let Some(ref stop) = provider.default_options.stop {
        output.push_str(&format!("  stop: {:?}\n", stop));
    }

    output
}

/// Format provider show as JSON
fn format_provider_show_json(provider: &crate::config::ProviderConfig, api_key_status: Option<&str>) -> String {
    use serde_json::json;

    let type_str = match provider.provider_type {
        crate::config::ProviderType::OpenAI => "openai",
        crate::config::ProviderType::Anthropic => "anthropic",
        crate::config::ProviderType::Ollama => "ollama",
        crate::config::ProviderType::LocalCustom => "local",
    };

    let api_key_status_str = api_key_status.map(|s| {
        match s {
            s if s.contains("from config") => "set_from_config",
            s if s.contains("from environment") => "set_from_env",
            s if s.contains("Not set") => "not_set",
            s if s.contains("Not required") => "not_required",
            _ => "unknown",
        }
    });

    let default_options = json!({
        "temperature": provider.default_options.temperature,
        "max_tokens": provider.default_options.max_tokens,
        "top_p": provider.default_options.top_p,
        "frequency_penalty": provider.default_options.frequency_penalty,
        "presence_penalty": provider.default_options.presence_penalty,
        "stop": provider.default_options.stop,
    });

    let result = json!({
        "provider_name": provider.provider_name.as_deref().unwrap_or("unknown"),
        "provider_type": type_str,
        "model": provider.model,
        "endpoint": provider.endpoint,
        "api_key_status": api_key_status_str,
        "default_options": default_options,
    });

    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Format provider validation result
fn format_provider_validation_result(result: &crate::provider::ValidationResult, verbose: bool) -> String {
    let mut output = format!("Validating provider: {}\n\n", result.provider_name);

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
            output.push_str("\nErrors:\n");
            for error in &result.errors {
                output.push_str(&format!("✗ {}\n", error));
            }
        }

        // Show warnings
        if !result.warnings.is_empty() {
            output.push_str("\nWarnings:\n");
            for warning in &result.warnings {
                output.push_str(&format!("⚠ {}\n", warning));
            }
        }

        output.push_str(&format!("\nValidation {}: {}/{} checks passed, {} errors found\n",
            if result.is_valid() { "passed" } else { "failed" },
            result.passed_checks(),
            result.total_checks(),
            result.errors.len()
        ));
    }

    if verbose {
        output.push_str(&format!("\nTotal checks: {}\n", result.total_checks()));
        output.push_str(&format!("Passed: {}\n", result.passed_checks()));
        output.push_str(&format!("Errors: {}\n", result.errors.len()));
        output.push_str(&format!("Warnings: {}\n", result.warnings.len()));
    }

    output
}

/// Format context output as text
fn format_context_text_output(
    context: &crate::api::NodeContext,
    include_metadata: bool,
    combine: bool,
    separator: &str,
    include_deleted: bool,
) -> Result<String, ApiError> {
    // Filter deleted frames if not including them
    let frames: Vec<&crate::frame::Frame> = if include_deleted {
        context.frames.iter().collect()
    } else {
        context.frames.iter()
            .filter(|f| !f.metadata.get("deleted").map(|v| v == "true").unwrap_or(false))
            .collect()
    };

    if frames.is_empty() {
        return Ok(format!(
            "Node: {}\nPath: {}\nNo frames found.",
            hex::encode(context.node_id),
            context.node_record.path.display()
        ));
    }

    if combine {
        // Concatenate all frame contents
        let texts: Vec<String> = frames.iter()
            .filter_map(|f| f.text_content().ok())
            .collect();
        Ok(texts.join(separator))
    } else {
        // Show frames individually
        let mut output = format!(
            "Node: {}\nPath: {}\nFrames: {}/{}\n\n",
            hex::encode(context.node_id),
            context.node_record.path.display(),
            frames.len(),
            context.frame_count
        );

        for (i, frame) in frames.iter().enumerate() {
            output.push_str(&format!("--- Frame {} ---\n", i + 1));
            
            if include_metadata {
                output.push_str(&format!("Frame ID: {}\n", hex::encode(frame.frame_id)));
                output.push_str(&format!("Frame Type: {}\n", frame.frame_type));
                if let Some(agent_id) = frame.agent_id() {
                    output.push_str(&format!("Agent: {}\n", agent_id));
                }
                output.push_str(&format!("Timestamp: {:?}\n", frame.timestamp));
                if !frame.metadata.is_empty() {
                    output.push_str("Metadata:\n");
                    for (key, value) in &frame.metadata {
                        if key != "agent_id" && key != "deleted" {
                            output.push_str(&format!("  {}: {}\n", key, value));
                        }
                    }
                }
                output.push_str("\n");
            }

            if let Ok(text) = frame.text_content() {
                output.push_str(&format!("Content:\n{}\n", text));
            } else {
                output.push_str("Content: [Binary content - not UTF-8]\n");
            }
            output.push_str("\n");
        }

        Ok(output)
    }
}

/// Format context output as JSON
fn format_context_json_output(
    context: &crate::api::NodeContext,
    include_metadata: bool,
    include_deleted: bool,
) -> Result<String, ApiError> {
    use serde_json::json;

    // Filter deleted frames if not including them
    let frames: Vec<&crate::frame::Frame> = if include_deleted {
        context.frames.iter().collect()
    } else {
        context.frames.iter()
            .filter(|f| !f.metadata.get("deleted").map(|v| v == "true").unwrap_or(false))
            .collect()
    };

    let frames_json: Vec<serde_json::Value> = frames.iter().map(|frame| {
        let mut frame_obj = json!({
            "frame_id": hex::encode(frame.frame_id),
            "frame_type": frame.frame_type,
            "timestamp": frame.timestamp,
        });

        if include_metadata {
            if let Some(agent_id) = frame.agent_id() {
                frame_obj["agent_id"] = json!(agent_id);
            }
            frame_obj["metadata"] = json!(frame.metadata);
        }

        if let Ok(text) = frame.text_content() {
            frame_obj["content"] = json!(text);
        } else {
            frame_obj["content"] = json!(null);
            frame_obj["content_binary"] = json!(true);
        }

        frame_obj
    }).collect();

    let result = json!({
        "node_id": hex::encode(context.node_id),
        "path": context.node_record.path.to_string_lossy(),
        "node_type": match context.node_record.node_type {
            crate::store::NodeType::File { size, .. } => format!("file:{}", size),
            crate::store::NodeType::Directory => "directory".to_string(),
        },
        "frames": frames_json,
        "frame_count": frames.len(),
        "total_frame_count": context.frame_count,
    });

    serde_json::to_string_pretty(&result)
        .map_err(|e| ApiError::ConfigError(format!("Failed to serialize JSON: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ContextApi;
    use crate::frame::storage::FrameStorage;
    use crate::heads::HeadIndex;
    use crate::regeneration::BasisIndex;
    use crate::store::persistence::SledNodeRecordStore;
    use crate::types::Hash;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_api() -> (ContextApi, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage_path = temp_dir.path().join("frames");
        std::fs::create_dir_all(&frame_storage_path).unwrap();
        let frame_storage = Arc::new(
            FrameStorage::new(&frame_storage_path).unwrap()
        );
        let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
        let basis_index = Arc::new(parking_lot::RwLock::new(BasisIndex::new()));
        let agent_registry = Arc::new(parking_lot::RwLock::new(crate::agent::AgentRegistry::new()));
        let provider_registry = Arc::new(parking_lot::RwLock::new(crate::provider::ProviderRegistry::new()));
        let lock_manager = Arc::new(crate::concurrency::NodeLockManager::new());

        let api = ContextApi::new(
            node_store,
            frame_storage,
            head_index,
            basis_index,
            agent_registry,
            provider_registry,
            lock_manager,
        );

        (api, temp_dir)
    }

    #[test]
    fn test_parse_node_id_valid() {
        let node_id = [1u8; 32];
        let hex_str = hex::encode(node_id);
        let parsed = parse_node_id(&hex_str).unwrap();
        assert_eq!(parsed, Hash::from(node_id));
    }

    #[test]
    fn test_parse_node_id_with_prefix() {
        let node_id = [1u8; 32];
        let hex_str = format!("0x{}", hex::encode(node_id));
        let parsed = parse_node_id(&hex_str).unwrap();
        assert_eq!(parsed, Hash::from(node_id));
    }

    #[test]
    fn test_parse_node_id_invalid() {
        // Invalid hex
        assert!(parse_node_id("not-hex").is_err());
        
        // Wrong length
        let short_hex = hex::encode([1u8; 16]);
        assert!(parse_node_id(&short_hex).is_err());
    }

    #[test]
    fn test_resolve_path_to_node_id() {
        let (api, temp_dir) = create_test_api();
        let workspace_root = temp_dir.path().to_path_buf();
        
        // Create a test node record
        let node_id: NodeID = [1u8; 32];
        let test_path = workspace_root.join("test.txt");
        std::fs::write(&test_path, "test content").unwrap();
        
        let canonical_path = crate::tree::path::canonicalize_path(&test_path).unwrap();
        
        let record = crate::store::NodeRecord {
            node_id,
            path: canonical_path.clone(),
            node_type: crate::store::NodeType::File {
                size: 12,
                content_hash: [0u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: std::collections::HashMap::new(),
        };
        
        api.node_store().put(&record).unwrap();
        
        // Test path resolution
        let resolved = resolve_path_to_node_id(&api, &test_path, &workspace_root).unwrap();
        assert_eq!(resolved, node_id);
    }

    #[test]
    fn test_resolve_path_to_node_id_not_found() {
        let (api, temp_dir) = create_test_api();
        let workspace_root = temp_dir.path().to_path_buf();
        
        // Create the file but don't add it to the store
        let test_path = workspace_root.join("nonexistent.txt");
        std::fs::write(&test_path, "test content").unwrap();
        
        let result = resolve_path_to_node_id(&api, &test_path, &workspace_root);
        assert!(result.is_err());
        match result {
            Err(ApiError::PathNotInTree(_)) => {}
            _ => panic!("Expected PathNotInTree error, got: {:?}", result),
        }
    }

    #[test]
    fn test_format_context_text_output_combine() {
        let node_id: NodeID = [1u8; 32];
        let node_record = crate::store::NodeRecord {
            node_id,
            path: std::path::PathBuf::from("/test/file.txt"),
            node_type: crate::store::NodeType::File {
                size: 100,
                content_hash: [0u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: std::collections::HashMap::new(),
        };

        let frame1 = crate::frame::Frame::new(
            crate::frame::Basis::Node(node_id),
            b"Frame 1 content".to_vec(),
            "type1".to_string(),
            "agent1".to_string(),
            std::collections::HashMap::new(),
        ).unwrap();

        let frame2 = crate::frame::Frame::new(
            crate::frame::Basis::Node(node_id),
            b"Frame 2 content".to_vec(),
            "type2".to_string(),
            "agent2".to_string(),
            std::collections::HashMap::new(),
        ).unwrap();

        let context = crate::api::NodeContext {
            node_id,
            node_record,
            frames: vec![frame1, frame2],
            frame_count: 2,
        };

        let output = format_context_text_output(&context, false, true, " | ", false).unwrap();
        assert!(output.contains("Frame 1 content"));
        assert!(output.contains("Frame 2 content"));
        assert!(output.contains(" | "));
    }

    #[test]
    fn test_format_context_json_output() {
        let node_id: NodeID = [1u8; 32];
        let node_record = crate::store::NodeRecord {
            node_id,
            path: std::path::PathBuf::from("/test/file.txt"),
            node_type: crate::store::NodeType::File {
                size: 100,
                content_hash: [0u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: std::collections::HashMap::new(),
        };

        let frame = crate::frame::Frame::new(
            crate::frame::Basis::Node(node_id),
            b"Test content".to_vec(),
            "test".to_string(),
            "agent1".to_string(),
            std::collections::HashMap::new(),
        ).unwrap();

        let context = crate::api::NodeContext {
            node_id,
            node_record,
            frames: vec![frame],
            frame_count: 1,
        };

        let output = format_context_json_output(&context, false, false).unwrap();
        assert!(output.contains("node_id"));
        assert!(output.contains("frames"));
        assert!(output.contains("Test content"));
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

/// Resolve a path to a NodeID
///
/// Canonicalizes the path relative to the workspace root and looks it up in the node store.
fn resolve_path_to_node_id(
    api: &ContextApi,
    path: &PathBuf,
    workspace_root: &PathBuf,
) -> Result<NodeID, ApiError> {
    // Resolve path relative to workspace root
    let resolved_path = if path.is_absolute() {
        path.clone()
    } else {
        workspace_root.join(path)
    };

    // Canonicalize the path
    let canonical_path = crate::tree::path::canonicalize_path(&resolved_path)
        .map_err(|e| ApiError::StorageError(e))?;

    // Look up NodeID in store
    match api.node_store().find_by_path(&canonical_path).map_err(ApiError::from)? {
        Some(record) => Ok(record.node_id),
        None => Err(ApiError::PathNotInTree(canonical_path)),
    }
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

/// Format initialization preview
fn format_init_preview(preview: &crate::init::InitPreview) -> String {
    let mut output = String::from("Initialization Preview:\n\n");
    
    if !preview.prompts.is_empty() {
        output.push_str("Would create prompts:\n");
        for prompt in &preview.prompts {
            output.push_str(&format!("  - {}\n", prompt));
        }
        output.push('\n');
    }
    
    if !preview.agents.is_empty() {
        output.push_str("Would create agents:\n");
        for agent in &preview.agents {
            output.push_str(&format!("  - {}.toml\n", agent));
        }
        output.push('\n');
    }
    
    if preview.prompts.is_empty() && preview.agents.is_empty() {
        output.push_str("All default agents and prompts already exist.\n");
    } else {
        output.push_str("Run 'merkle init' to perform initialization.\n");
    }
    
    output
}

/// Format initialization summary
fn format_init_summary(summary: &crate::init::InitSummary, force: bool) -> String {
    let mut output = String::from("Initializing Merkle configuration...\n\n");
    
    // Prompts section
    if !summary.prompts.created.is_empty() || !summary.prompts.skipped.is_empty() {
        let prompts_dir = crate::config::xdg::prompts_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "~/.config/merkle/prompts/".to_string());
        output.push_str(&format!("Created prompts directory: {}\n", prompts_dir));
        
        for prompt in &summary.prompts.created {
            if force {
                output.push_str(&format!("  ✓ {} (overwritten)\n", prompt));
            } else {
                output.push_str(&format!("  ✓ {}\n", prompt));
            }
        }
        for prompt in &summary.prompts.skipped {
            output.push_str(&format!("  ⊘ {} (already exists, skipped)\n", prompt));
        }
        output.push('\n');
    }
    
    // Agents section
    if !summary.agents.created.is_empty() || !summary.agents.skipped.is_empty() {
        let agents_dir = crate::config::xdg::agents_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "~/.config/merkle/agents/".to_string());
        output.push_str(&format!("Created agents directory: {}\n", agents_dir));
        
        for agent in &summary.agents.created {
            let role_str = match agent.as_str() {
                "reader" => "Reader",
                "code-analyzer" => "Writer",
                "docs-writer" => "Writer",
                "synthesis-agent" => "Synthesis",
                _ => "Unknown",
            };
            if force {
                output.push_str(&format!("  ✓ {}.toml ({}) (overwritten)\n", agent, role_str));
            } else {
                output.push_str(&format!("  ✓ {}.toml ({})\n", agent, role_str));
            }
        }
        for agent in &summary.agents.skipped {
            let role_str = match agent.as_str() {
                "reader" => "Reader",
                "code-analyzer" => "Writer",
                "docs-writer" => "Writer",
                "synthesis-agent" => "Synthesis",
                _ => "Unknown",
            };
            output.push_str(&format!("  ⊘ {}.toml ({}) (already exists, skipped)\n", agent, role_str));
        }
        output.push('\n');
    }
    
    // Errors section
    if !summary.prompts.errors.is_empty() || !summary.agents.errors.is_empty() {
        output.push_str("Errors:\n");
        for error in &summary.prompts.errors {
            output.push_str(&format!("  ✗ {}\n", error));
        }
        for error in &summary.agents.errors {
            output.push_str(&format!("  ✗ {}\n", error));
        }
        output.push('\n');
    }
    
    // Validation section
    let all_valid = summary.validation.results.iter().all(|(_, is_valid, _)| *is_valid);
    if all_valid {
        output.push_str("Validation:\n");
        output.push_str("  ✓ All agents validated successfully\n\n");
    } else {
        output.push_str("Validation:\n");
        for (agent_id, is_valid, errors) in &summary.validation.results {
            if *is_valid {
                output.push_str(&format!("  ✓ {} validated\n", agent_id));
            } else {
                output.push_str(&format!("  ✗ {} validation failed:\n", agent_id));
                for error in errors {
                    output.push_str(&format!("    - {}\n", error));
                }
            }
        }
        output.push('\n');
    }
    
    if summary.prompts.created.is_empty() && summary.agents.created.is_empty() && !force {
        output.push_str("All default agents already exist. Use --force to re-initialize.\n");
    } else {
        output.push_str("Initialization complete! You can now use:\n");
        output.push_str("  - merkle agent list          # List all agents\n");
        output.push_str("  - merkle agent show <id>     # View agent details\n");
        output.push_str("  - merkle context generate    # Generate context frames\n");
    }
    
    output
}
